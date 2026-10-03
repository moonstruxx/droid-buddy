use std::time::{Duration, Instant};

use crate::app::Rect;
use egui::{Modifiers, PointerButton};
use winit::event::WindowEvent;
use winit::keyboard::NamedKey;

#[cfg(test)]
use crate::gui::{camera_zoom_about, MAX_ZOOM_STEP, ZOOM_SENSITIVITY};
#[cfg(test)]
use crate::gui::{PanelsFrame, PhysicalFrame, PickerFrame, ViewerFrame};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyEvent {
    pub fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }

    /// Convert a winit key event into the neutral shape the handler binds.
    /// Convert the parts of a winit keyboard event into the neutral shape the
    /// handler binds. Produced characters become `Char` (shifted glyphs arrive
    /// as their symbol, e.g. `?` for Shift+/), named keys map to the matching
    /// code, and Shift+Tab becomes `BackTab`.
    /// Control maps to ctrl+command (the handler treats the pair as one), Super
    /// to mac_cmd only. Multi-character composition and unbound keys map to
    /// `None`, as do release events. winit reports modifiers via
    /// `ModifiersChanged` rather than on the key event, so the loop tracks the
    /// current state and passes it in. The parts API exists because
    /// `winit::event::KeyEvent` carries a private field and cannot be
    /// constructed outside winit.
    pub fn from_winit_parts(
        logical_key: &winit::keyboard::Key,
        state: winit::event::ElementState,
        winit_mods: winit::keyboard::ModifiersState,
    ) -> Option<Self> {
        if state != winit::event::ElementState::Pressed {
            return None;
        }
        let code = match logical_key {
            winit::keyboard::Key::Character(ch) => {
                let mut chars = ch.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                KeyCode::Char(c)
            }
            winit::keyboard::Key::Named(named) => match named {
                NamedKey::Enter => KeyCode::Enter,
                NamedKey::Escape => KeyCode::Esc,
                NamedKey::Backspace => KeyCode::Backspace,
                NamedKey::Tab if winit_mods.shift_key() => KeyCode::BackTab,
                NamedKey::Tab => KeyCode::Tab,
                NamedKey::Space => KeyCode::Char(' '),
                NamedKey::ArrowUp => KeyCode::Up,
                NamedKey::ArrowDown => KeyCode::Down,
                NamedKey::ArrowLeft => KeyCode::Left,
                NamedKey::ArrowRight => KeyCode::Right,
                NamedKey::Home => KeyCode::Home,
                NamedKey::End => KeyCode::End,
                _ => return None,
            },
            _ => return None,
        };
        let mut modifiers = Modifiers::NONE;
        if winit_mods.shift_key() {
            modifiers.shift = true;
        }
        if winit_mods.control_key() {
            modifiers.ctrl = true;
            modifiers.command = true;
        }
        if winit_mods.alt_key() {
            modifiers.alt = true;
        }
        if winit_mods.super_key() {
            modifiers.mac_cmd = true;
        }
        Some(Self::new(code, modifiers))
    }
}

pub mod key_modifiers {
    use egui::Modifiers;
    pub const NONE: Modifiers = Modifiers::NONE;
    pub const CONTROL: Modifiers = Modifiers {
        ctrl: true,
        alt: false,
        shift: false,
        mac_cmd: false,
        command: true,
    };
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        alt: false,
        ctrl: false,
        mac_cmd: false,
        command: false,
    };
    pub const ALT: Modifiers = Modifiers {
        alt: true,
        ctrl: false,
        shift: false,
        mac_cmd: false,
        command: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl From<MouseButton> for PointerButton {
    fn from(b: MouseButton) -> Self {
        match b {
            MouseButton::Left => PointerButton::Primary,
            MouseButton::Right => PointerButton::Secondary,
            MouseButton::Middle => PointerButton::Middle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    Moved,
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: Modifiers,
}

fn rect_contains(rect: &Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

#[cfg(test)]
pub(crate) fn handle_physical_frame(frame: PhysicalFrame, app: &mut crate::app::App) {
    if frame.skeleton_toggle {
        app.physical_show_skeleton = !app.physical_show_skeleton;
        app.status_message = if app.physical_show_skeleton {
            String::from("Skeleton: on")
        } else {
            String::from("Skeleton: off")
        };
    }
    if frame.pan_delta != (0.0, 0.0) {
        app.physical_offset.0 += frame.pan_delta.0;
        app.physical_offset.1 += frame.pan_delta.1;
    }
    if let Some((factor, anchor)) = frame.zoom {
        let clamped = factor.clamp(1.0 / MAX_ZOOM_STEP, MAX_ZOOM_STEP);
        if let Some(cam) = app.graph_camera.as_mut() {
            let next = camera_zoom_about(cam, clamped, anchor);
            *cam = next;
        } else {
            app.physical_zoom = (app.physical_zoom * clamped).clamp(0.5, 3.0);
            app.scale_factor = app.physical_zoom;
        }
        let _ = ZOOM_SENSITIVITY;
    }
    let _ = WindowEvent::RedrawRequested;
}

#[cfg(test)]
pub(crate) fn handle_panels_frame(frame: PanelsFrame, app: &mut crate::app::App) {
    app.hovered_component = frame.hovered;
    if let Some(idx) = frame.clicked {
        let token = app
            .patch
            .as_ref()
            .and_then(|p| p.hw_components.get(idx))
            .map(|c| c.id.clone());
        if let Some(token) = token {
            if !app.processing_paused {
                if let Some(patch) = &mut app.patch {
                    if let Some(comp) = patch.hw_components.get_mut(idx) {
                        toggle_component(comp);
                        app.status_message = format!("Toggled: {}", comp.label);
                    }
                }
            }
            app.select_component(token);
        }
        app.open_view(crate::app::ViewType::Physical);
    }
    if let Some(delta) = frame.scroll {
        if let Some(idx) = frame.hovered {
            if !app.processing_paused {
                if let Some(patch) = &mut app.patch {
                    if let Some(comp) = patch.hw_components.get_mut(idx) {
                        let delta_val = delta * ZOOM_SENSITIVITY * 10.0;
                        if let crate::patch::ComponentState::Value(v) = comp.state {
                            comp.state = crate::patch::ComponentState::Value(
                                (v + delta_val).clamp(0.0, 1.0),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn handle_viewer_frame(frame: ViewerFrame, app: &mut crate::app::App) {
    if frame.scroll_delta != 0.0 {
        let delta = frame.scroll_delta as isize;
        if delta > 0 {
            app.source_scroll = app.source_scroll.saturating_add(delta as usize);
        } else {
            app.source_scroll = app.source_scroll.saturating_sub((-delta) as usize);
        }
    }
}

#[cfg(test)]
pub(crate) fn handle_picker_frame(frame: PickerFrame, app: &mut crate::app::App) {
    if let Some(hovered) = frame.hovered {
        app.picker_index = hovered;
    }
    if let Some(clicked) = frame.clicked {
        if let Some(path) = app.picker_entries.get(clicked).cloned() {
            open_picker_entry(app, path);
        }
    }
}

pub fn handle_window_event(
    event: &WindowEvent,
    modifiers: winit::keyboard::ModifiersState,
    app: &mut crate::app::App,
) -> bool {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            handle_window_key_event(&event.logical_key, event.state, modifiers, app)
        }
        _ => false,
    }
}

/// Convert one winit keyboard event and run the shared dispatch. Returns
/// whether the handler asked to quit. Takes the event's parts so tests can
/// drive the conversion without constructing a winit `DeviceId` or `KeyEvent`
/// (the latter has a private field).
pub fn handle_window_key_event(
    logical_key: &winit::keyboard::Key,
    state: winit::event::ElementState,
    modifiers: winit::keyboard::ModifiersState,
    app: &mut crate::app::App,
) -> bool {
    // from_winit_parts filters release events; presses convert to the neutral
    // shape and run the same dispatch as the terminal key path.
    KeyEvent::from_winit_parts(logical_key, state, modifiers)
        .is_some_and(|key| handle_event(key, app))
}

use std::collections::HashMap;
use std::path::PathBuf;

use crate::app::{
    is_entry_selectable, is_picker_parent_entry, App, GraphDrag, PrefixState, SourceViewMode,
    ViewType, ViewerFocus,
};
use crate::layout;
use crate::patch::{ComponentKind, ComponentState, HwComponent, NodeId, Patch, ShiftGroup};

/// How long an armed `g` prefix waits for its follow-up key before silently
/// cancelling. The timeout is lazy: it is checked only when the next event
/// arrives, so no timer thread or event-loop change is needed.
const PREFIX_TIMEOUT: Duration = Duration::from_secs(1);

/// Compat mirror: `viewer_focus` follows the focused pane so pre-class-layout
/// consumers (gui viewer focus, source navigation) stay in sync. The class
/// layout is the source of truth; this derives the legacy enum from it.
fn sync_viewer_focus_from_tiles(app: &mut App) {
    app.viewer_focus = if focused_view(app) == Some(ViewType::SourceViewer) {
        ViewerFocus::Source
    } else {
        ViewerFocus::Panels
    };
}

/// The view of the focused pane, or `None` when the focused pane is empty
/// (spec "Focus routing across panes": keyboard input routes to the focused
/// pane's view). This is the single focus source for the per-view key sets.
fn focused_view(app: &App) -> Option<ViewType> {
    app.layout.pane(app.layout.focus).view
}

/// True when keys should act on the graph pane: the focused pane shows the
/// signal-flow graph (spec "Keys route to the focused view").
fn graph_slot_focused(app: &App) -> bool {
    focused_view(app) == Some(ViewType::Graph)
}

/// True when keys should act on the optimizer pane: the focused pane shows the
/// latency optimizer.
fn optimizer_slot_focused(app: &App) -> bool {
    focused_view(app) == Some(ViewType::Optimizer)
}

/// True when keys should act on the Physical pane: the focused pane shows the
/// rack view.
fn physical_slot_focused(app: &App) -> bool {
    focused_view(app) == Some(ViewType::Physical)
}

/// Cycle the physical panel scale presets (shared by the panels arm and the
/// graph arm's scale-the-other-pane path).
fn cycle_panel_scale(app: &mut App, plus: bool) {
    // Cycle through the scaling presets defined by the module-scaling spec.
    const PRESETS: [f32; 4] = [0.75, 1.0, 1.5, 2.0];
    let idx = PRESETS
        .iter()
        .position(|p| (*p - app.scale_factor).abs() < f32::EPSILON)
        .unwrap_or(1);
    let step = if plus { 1 } else { PRESETS.len() - 1 };
    let next = PRESETS[(idx + step) % PRESETS.len()];
    app.scale_factor = next;
    // The physical renderers link `physical_zoom` from `scale_factor`
    // every frame; sync it here so the status hint below shows the
    // new zoom immediately instead of lagging one frame.
    app.physical_zoom = next;
    app.status_message = app
        .physical_status_hint()
        .unwrap_or_else(|| format!("Scaling: {}%", (next * 100.0) as u32));
}

/// Focus the pane holding `view` (no-op when the view is not open). The class
/// layout owns placement, so `open_view` on an already-open view only moves
/// focus to that pane.
fn focus_tile_slot(app: &mut App, view: ViewType) {
    if app.pane_holding(view).is_some() {
        app.open_view(view);
    }
}

/// Focus the pane under the pointer, if any (spec "Focus routing across
/// panes": a mouse click inside a pane focuses that pane). An empty pane has
/// no view to focus, so it is left alone.
fn focus_pane_at(app: &mut App, column: u16, row: u16) {
    let Some((id, _)) = app
        .pane_hit_rects
        .iter()
        .find(|(_, rect)| rect_contains(rect, column, row))
    else {
        return;
    };
    if let Some(view) = app.layout.pane(*id).view {
        app.open_view(view);
    }
}

/// `[`/`]`: move the left/right big-pane boundary in 10% steps, clamped to
/// 30–70% (spec "Adaptive pane layout"). Returns the status line for the
/// resulting split; the ratio is snapped so repeated presses stay exact.
fn adjust_big_boundary(app: &mut App, delta: f32) -> String {
    app.adjust_main_split_ratio(delta);
    let ratio = (app.layout.main_split_ratio * 10.0).round() / 10.0;
    app.layout.main_split_ratio = ratio;
    app.main_split_ratio = ratio as f32;
    app.viewer_split_ratio = ratio as f32;
    format!("Split: {:.0}%/{:.0}%", ratio * 100.0, 100.0 - ratio * 100.0)
}

/// `Alt+[`/`Alt+]`: move the boundary between the two small panes in 10%
/// steps, clamped to 30–70% (spec "Adaptive pane layout"). Returns the status
/// line for the resulting split.
fn adjust_small_boundary(app: &mut App, delta: f32) -> String {
    app.adjust_small_split_ratio(delta);
    let ratio = (app.layout.small_split_ratio * 10.0).round() / 10.0;
    app.layout.small_split_ratio = ratio;
    format!(
        "Small split: {:.0}%/{:.0}%",
        ratio * 100.0,
        100.0 - ratio * 100.0
    )
}

fn open_embedded_viewer(app: &mut App) {
    app.showing_viewer = true;
    app.viewer_focus = ViewerFocus::Source;
    app.prefix = None;
    // Initial-position rule: BOF when nothing selected, else first occurrence
    // of the selected component.
    if let Some(token) = app.selected_component.clone() {
        if let Some(patch) = app.patch.as_ref() {
            if let Some(spans) = patch.occurrence_index.get(&token) {
                if let Some(first) = spans.first() {
                    app.source_scroll = first.line;
                    app.occurrence_cursor = 0;
                    return;
                }
            }
        }
        // Selected token has no occurrence: fall through to BOF but keep
        // selection and reset cursor.
        app.source_scroll = 0;
        app.occurrence_cursor = 0;
    } else {
        app.source_scroll = 0;
        app.occurrence_cursor = 0;
    }
}

/// Handle keyboard input. Returns true if the app should quit.
/// Handle keyboard input. Returns true if the app should quit.
pub fn handle_event(key: KeyEvent, app: &mut App) -> bool {
    // Inline label-edit overlay eats all keys (highest priority: overlay > picker > prefix > graph > source > panels).
    if app.editing.is_some() {
        match key.code {
            KeyCode::Esc => {
                app.cancel_edit();
                app.status_message = String::from("Edit cancelled");
                return false;
            }
            KeyCode::Enter => {
                match app.commit_edit() {
                    Ok(()) => app.status_message = String::from("Label saved"),
                    Err(e) => app.status_message = format!("Save failed: {e}"),
                }
                return false;
            }
            KeyCode::Backspace => {
                if let Some(state) = app.editing.as_mut() {
                    state.draft.pop();
                }
                return false;
            }
            KeyCode::Char(c)
                if key.modifiers.is_none() || key.modifiers == key_modifiers::SHIFT =>
            {
                if c.is_ascii_digit() && c != '0' {
                    let settings = crate::config::load(
                        &crate::theme::canonical_theme_name,
                        crate::theme::THEMES,
                    );
                    let max = if settings.labels.layers_enabled {
                        settings.labels.max_shift_layer.clamp(1, 8)
                    } else {
                        1
                    };
                    let digit = c.to_digit(10).unwrap() as u8;
                    if (1..=max).contains(&digit) && app.cycle_edit_layer(digit) {
                        if let Some(line) = app.editing_status_line(
                            settings.labels.layers_enabled,
                            settings.labels.max_shift_layer,
                        ) {
                            app.status_message = line;
                        }
                        return false;
                    }
                }
                if let Some(state) = app.editing.as_mut() {
                    state.draft.push(c);
                }
                return false;
            }
            _ => return false,
        }
    }
    // Help modal (design D1): sits directly below the edit overlay in the
    // priority chain (overlay > help > picker > validation > optimizer > ...).
    // While open it eats all keys except Esc and q, both of which close it and
    // return false — so `q` closes help instead of quitting, scoped to the
    // modal's lifetime.
    if app.showing_help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.close_help();
                return false;
            }
            _ => return false,
        }
    }
    // `?` opens the help modal from any view (design D2). Matched without a
    // modifier guard because the key arrives with SHIFT (Shift+/), the same
    // convention as `+` (Shift+=). The edit overlay above already eats it.
    if matches!(key.code, KeyCode::Char('?')) {
        app.open_help();
        return false;
    }
    // If file picker is showing, handle picker navigation
    if app.showing_picker {
        return handle_picker_event(key, app);
    }

    // Validation modal overlay: priority third (overlay > picker > validation).
    // When open it eats all keys; j/k navigate, Esc/e close, Enter jumps to source.
    if app.showing_validation {
        match key.code {
            KeyCode::Esc => {
                app.showing_validation = false;
                return false;
            }
            KeyCode::Char('e') if key.modifiers.is_none() => {
                app.showing_validation = false;
                return false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if app.validation_cursor + 1 < app.validation_issues.len() {
                    app.validation_cursor += 1;
                }
                return false;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.validation_cursor > 0 {
                    app.validation_cursor -= 1;
                }
                return false;
            }
            KeyCode::Enter => {
                if let Some(issue) = app.validation_issues.get(app.validation_cursor).cloned() {
                    app.source_scroll = issue.span.line;
                    // Open the source viewer pane and focus it so the jumped
                    // span is visible (class-routed open).
                    app.showing_viewer = true;
                    app.open_view(ViewType::SourceViewer);
                    sync_viewer_focus_from_tiles(app);
                    app.showing_validation = false;
                }
                return false;
            }
            _ => return false,
        }
    }
    // `e` toggles validation modal open when not already showing and issues exist.
    // Respects label-edit priority: if a hovered node/component or source header
    // would consume `e` for label editing, let that handler run instead.
    if matches!(key.code, KeyCode::Char('e'))
        && key.modifiers.is_none()
        && !app.validation_issues.is_empty()
        && !app.showing_validation
    {
        let has_label_target = app.hovered_graph_node.is_some()
            || app.hovered_component.is_some()
            || focused_view(app) == Some(ViewType::SourceViewer);
        if !has_label_target {
            app.showing_validation = true;
            if app.validation_cursor >= app.validation_issues.len() {
                app.validation_cursor = 0;
            }
            return false;
        }
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
            KeyCode::Char('v') => {
                open_embedded_viewer(app);
                // Class-routed open: the source viewer takes a small pane and
                // focus with it.
                app.open_view(ViewType::SourceViewer);
                sync_viewer_focus_from_tiles(app);
                return false;
            }
            KeyCode::Char('g') => {
                // `g g` opens the graph surface in a big pane.
                app.open_graph();
                // Focus follows it so Esc/keys act on the graph pane.
                focus_tile_slot(app, ViewType::Graph);
                sync_viewer_focus_from_tiles(app);
                app.prefix = None;
                return false;
            }
            KeyCode::Char('d') => {
                // `g d` opens picker for B patch (patch-diff-viewer).
                app.diff_picker_active = true;
                app.showing_picker = true;
                if app.picker_dir.as_os_str().is_empty() {
                    app.picker_dir = std::env::current_dir().unwrap_or_default();
                }
                app.picker_index = 0;
                app.refresh_picker_entries();
                app.prefix = None;
                return false;
            }
            KeyCode::Char('o') => {
                // `g o` opens the optimizer as a right-column pane (change
                // `tiled-window-manager`, 5.1): `open_view` handles the
                // already-open focus case and the slot cap, and lands focus
                // on the pane.
                app.open_view(ViewType::Optimizer);
                sync_viewer_focus_from_tiles(app);
                app.prefix = None;
                return false;
            }
            KeyCode::Char('c') => {
                // `g c` toggles cable latency coloring. The bare `c` key now
                // centers the graph (design D3), so latency coloring lives on
                // this chord only.
                app.toggle_latency_coloring();
                app.prefix = None;
                return false;
            }
            KeyCode::Char('q') => {
                // Quad view is retired with the class layout: `g q` is an
                // unbound prefix follow-up, so it just cancels the prefix.
                app.prefix = None;
                return false;
            }
            KeyCode::Char('s') => {
                if app.open_select_menu() {
                    app.prefix = None;
                    return false;
                }
                app.prefix = None;
                return false;
            }
            KeyCode::Esc => {
                app.prefix = None;
                return false;
            }
            _ => {
                app.prefix = None;
            }
        }
    }

    // Diff overlay Esc handling: clear scope first, then overlay (before viewer/graph Esc).
    if matches!(key.code, KeyCode::Esc) {
        if app.diff_scope.is_some() {
            app.diff_scope = None;
            app.status_message = String::from("Diff scope cleared");
            app.prefix = None;
            return false;
        }
        if app.diff_showing {
            app.diff_showing = false;
            app.status_message = String::from("Diff hidden");
            app.prefix = None;
            return false;
        }
    }

    // Select-state menu (change C 4.1): when active, j/k navigate cursor,
    // [/] cycle candidates with live rebuild, Esc clears.
    if app.select_state.is_some() {
        match key.code {
            KeyCode::Esc => {
                app.close_select_menu();
                return false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(st) = app.select_state.as_mut() {
                    let n = st.signals.len();
                    if n > 0 && st.cursor + 1 < n {
                        st.cursor += 1;
                    }
                }
                return false;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(st) = app.select_state.as_mut() {
                    if st.cursor > 0 {
                        st.cursor -= 1;
                    }
                }
                return false;
            }
            KeyCode::Char('[') => {
                app.cycle_select_candidate(-1);
                return false;
            }
            KeyCode::Char(']') => {
                app.cycle_select_candidate(1);
                return false;
            }
            KeyCode::Enter => {
                app.cycle_select_candidate(1);
                return false;
            }
            _ => {}
        }
    }

    // Dependency-filter Esc handling (change D task 2.1): Esc clears the
    // filter before the tiled Esc that would close the focused view.
    if matches!(key.code, KeyCode::Esc) && app.dependency_root.is_some() {
        app.clear_dependency_filter();
        app.prefix = None;
        return false;
    }

    // Class-layout pane keys (change `pane-class-layout`, task 3.1). Priority
    // is overlay (help / validation / label editor) > picker > armed `g`
    // prefix > these keys > the per-view routing below, so `z`, `Alt+b`, and
    // `Alt+s` act on the band from any focused pane but never while the edit
    // overlay, help modal, validation modal, or picker has focus (spec
    // "Overlays take priority"). The select-state menu is a centered overlay
    // too and consumes them (spec "Overlays render above the pane layout").
    if app.select_state.is_none() {
        if key.code == KeyCode::Esc && key.modifiers.is_none() {
            // Spec "Esc clears maximize first": restore the arrangement
            // instead of closing the focused view; the view stays open. This
            // path also covers the mirror's empty-slot case where the tiled
            // dispatch below would never run.
            if app.layout.maximized.is_some() {
                app.layout.maximized = None;
                app.status_message = String::from("Layout restored");
                app.prefix = None;
                return false;
            }
        } else if key.modifiers.alt {
            match key.code {
                KeyCode::Char('b') => {
                    app.swap_big();
                    return false;
                }
                KeyCode::Char('s') => {
                    app.swap_small();
                    return false;
                }
                _ => {}
            }
        } else if key.code == KeyCode::Char('z') {
            app.maximize_toggle();
            return false;
        }
    }

    // Focus routing across panes (spec "Focus routing across panes"): exactly
    // one pane is focused; `Tab`/`Shift+Tab` cycle focus forward/backward in
    // tree order (skipping empty panes), and `Esc` closes the focused pane's
    // view. The maximize-restore, overlay, filter, and armed-prefix Esc paths
    // all returned above.
    let shift = key.modifiers.shift;
    if key.code == KeyCode::Tab && !shift {
        app.cycle_focus(true);
        sync_viewer_focus_from_tiles(app);
        return false;
    }
    if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
        app.cycle_focus(false);
        sync_viewer_focus_from_tiles(app);
        return false;
    }
    if key.code == KeyCode::Esc && key.modifiers.is_none() {
        // Spec "Esc closes the focused view": a view pane (graph, optimizer,
        // source viewer) closes, leaving its pane empty. The module UI is the
        // physical rack view (module-ui design D1) and keeps the old Panels
        // modifier-wash meaning: a held shift group or latched modifier clears
        // first, otherwise the module UI closes like any other view.
        match focused_view(app) {
            Some(ViewType::Graph | ViewType::Optimizer | ViewType::SourceViewer) => {
                let closed_viewer = focused_view(app) == Some(ViewType::SourceViewer);
                app.close_focused_view();
                if closed_viewer {
                    app.viewer_focus = ViewerFocus::Panels;
                }
            }
            _ => {
                if app.active_shift.is_some() || app.latched_component.is_some() {
                    app.active_shift = None;
                    if app.latched_component.is_some() {
                        app.latched_component = None;
                        app.refresh_modifier_influence();
                        app.status_message = String::from("Shift and modifier cleared");
                    } else {
                        app.status_message = String::from("Shift cleared");
                    }
                } else {
                    app.close_focused_view();
                }
            }
        }
        app.prefix = None;
        return false;
    }

    // Optimizer pane (change `tiled-window-manager`, 5.1): the optimizer is
    // a right-column view, so its keys respond only while the optimizer slot
    // holds focus (mirroring the graph/viewer pane dispatch). Esc already
    // closed the slot above via `close_focused_view`; unhandled keys fall
    // through so q/Ctrl+C quit and `l` still opens the picker.
    if optimizer_slot_focused(app) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let len = app
                    .optimizer
                    .as_ref()
                    .map(|s| s.candidates.len())
                    .unwrap_or(0);
                if let Some(state) = app.optimizer.as_mut() {
                    if state.cursor + 1 < len {
                        state.cursor += 1;
                    }
                }
                return false;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(state) = app.optimizer.as_mut() {
                    if state.cursor > 0 {
                        state.cursor -= 1;
                    }
                }
                return false;
            }
            KeyCode::Enter => {
                let idx = app.optimizer.as_ref().map(|s| s.cursor).unwrap_or(0);
                app.optimizer_preview(idx);
                return false;
            }
            KeyCode::Char('r') => {
                app.optimizer_restore();
                return false;
            }
            KeyCode::Char('s') => {
                let idx = app.optimizer.as_ref().map(|s| s.cursor).unwrap_or(0);
                app.optimizer_export(idx);
                return false;
            }
            // Weight slider (design D5): `[`/`]` step ±0.1 in [0,1], `0`/`1`
            // snap to the endpoints. Returned here so the split-ratio
            // handlers never see them while the optimizer pane is focused.
            KeyCode::Char('[') => {
                if let Some(state) = app.optimizer.as_ref() {
                    app.optimizer_set_weight(state.weight - 0.1);
                }
                return false;
            }
            KeyCode::Char(']') => {
                if let Some(state) = app.optimizer.as_ref() {
                    app.optimizer_set_weight(state.weight + 0.1);
                }
                return false;
            }
            KeyCode::Char('0') => {
                app.optimizer_set_weight(0.0);
                return false;
            }
            KeyCode::Char('1') => {
                app.optimizer_set_weight(1.0);
                return false;
            }
            _ => {}
        }
    }

    // Label edit overlay entry: `e` (lowercase, no mods) with priority graph hover > source header > panel hover.
    if matches!(key.code, KeyCode::Char('e')) && key.modifiers.is_none() && app.patch.is_some() {
        // 1) Graph hovered node -> Circuit
        if let Some(idx) = app.hovered_graph_node {
            if begin_graph_node_edit(app, idx) {
                return false;
            }
        }
        // 2) Source header focused -> Circuit instance at source_scroll
        let source_focused = focused_view(app) == Some(ViewType::SourceViewer);
        if source_focused {
            if let Some(patch) = app.patch.as_ref() {
                let line = app.source_scroll;
                let mut chosen: Option<usize> = None;
                for (i, sec) in patch.sections.iter().enumerate() {
                    if sec.header_span.line <= line {
                        chosen = Some(i);
                    } else {
                        break;
                    }
                }
                if let Some(idx) = chosen {
                    let name = patch.sections[idx].name.clone();
                    let mut counts: HashMap<String, usize> = HashMap::new();
                    let mut node: Option<NodeId> = None;
                    for (i, sec) in patch.sections.iter().enumerate() {
                        let entry = counts.entry(sec.name.clone()).or_insert(0);
                        if i == idx {
                            node = Some(NodeId::circuit(&name, *entry));
                            break;
                        }
                        *entry += 1;
                    }
                    if let Some(nid) = node {
                        let draft = app
                            .current_patch_path
                            .as_ref()
                            .and_then(|p| app.label_store.circuit_label(p, &nid))
                            .unwrap_or_default();
                        app.editing = Some(crate::app::EditState::new_circuit(nid.clone(), draft));
                        let settings2 = crate::config::load(
                            &crate::theme::canonical_theme_name,
                            crate::theme::THEMES,
                        );
                        if let Some(line) = app.editing_status_line(
                            settings2.labels.layers_enabled,
                            settings2.labels.max_shift_layer,
                        ) {
                            app.status_message = line;
                        } else {
                            app.status_message =
                                format!("Editing circuit {}:{}", nid.name(), nid.instance());
                        }
                        return false;
                    }
                }
            }
        }
        // 3) Panel hovered component -> HW token with current shift layer
        if let Some(hover) = app.hovered_component {
            if let Some(patch) = app.patch.as_ref() {
                if let Some(comp) = patch.hw_components.get(hover) {
                    let token = comp.id.clone();
                    let settings = crate::config::load(
                        &crate::theme::canonical_theme_name,
                        crate::theme::THEMES,
                    );
                    let max = settings.labels.max_shift_layer.clamp(1, 8);
                    let raw_layer = match app.active_shift {
                        Some(crate::patch::ShiftGroup::Group1) => 1,
                        Some(crate::patch::ShiftGroup::Group2) => 2,
                        Some(crate::patch::ShiftGroup::Group3) => 3,
                        Some(crate::patch::ShiftGroup::Group4) => 4,
                        None => 1,
                    };
                    let layer = if settings.labels.layers_enabled {
                        raw_layer.clamp(1, max)
                    } else {
                        1
                    };
                    let draft = app
                        .current_patch_path
                        .as_ref()
                        .and_then(|p| app.label_store.hw_label(p, &token, layer))
                        .unwrap_or_default();
                    app.editing = Some(crate::app::EditState::new_hw(token.clone(), layer, draft));
                    if let Some(line) = app.editing_status_line(
                        settings.labels.layers_enabled,
                        settings.labels.max_shift_layer,
                    ) {
                        app.status_message = line;
                    } else {
                        app.status_message = format!("Editing {} / Group{}", token, layer);
                    }
                    return false;
                }
            }
        }
    }

    // Graph surface handling (`g g`), routed only while the graph pane holds
    // focus (spec "Keys route to the focused view"). Global keys (`q`,
    // Ctrl+C, `l`) fall through to the shared dispatch below.
    if graph_slot_focused(app) {
        match key.code {
            KeyCode::Char('p') => {
                // Task 3.1 (design D7): `p` toggles pin/unpin on the hovered
                // graph node, mirroring the `x` processing toggle. Silent
                // no-op without hover; the rebuild + re-solve re-anchors the
                // layout. Processing pause stays on `p` on every other
                // surface.
                if let Some(idx) = app.hovered_graph_node {
                    graph_node_pin_toggle(app, idx);
                }
                return false;
            }
            KeyCode::Char('c') => {
                // Bare `c` centers the graph in the visible pane (design D3);
                // Ctrl+C (quit) is matched above with its modifier guard.
                app.center_graph_camera();
                return false;
            }
            KeyCode::Char('C') => {
                // Shift+c (winit reports Shift+c as `Char('C')`): refit the
                // camera to the published pane size and reset the preset to
                // the fitted zoom (design D3/D4).
                let viewport = app.graph_canvas_px.unwrap_or((1280.0, 800.0));
                app.fit_graph_camera(viewport);
                app.status_message = format!(
                    "Graph zoom {:.0}%",
                    App::GRAPH_ZOOM_PRESETS[App::GRAPH_ZOOM_FIT_INDEX] * 100.0
                );
                return false;
            }
            KeyCode::Char('x') => {
                if let Some(idx) = app.hovered_graph_node {
                    graph_node_processing_toggle(app, idx);
                }
                return false;
            }
            KeyCode::Char('f') => {
                // Change D task 2.1: `f` toggles the upstream dependency
                // filter rooted at the hovered node (fallback: the shared
                // circuit selection); a second `f` restores the full graph.
                app.toggle_dependency_filter();
                return false;
            }
            KeyCode::Char('i') => {
                // Change D task 2.1: `i` toggles the influence-filtered induced
                // subgraph rooted at the hovered node (fallback: the shared
                // circuit selection); a second `i` restores the full graph.
                app.toggle_influence_filter();
                return false;
            }
            KeyCode::Char('h') if graph_slot_focused(app) => {
                // graph-column-layout D5: `h` toggles between the deterministic
                // column arrangement and the force solver on the focused graph
                // pane. Flipping the mode then rebuilding re-solves under the
                // new arrangement (`solve_graph_positions` dispatches by mode)
                // and emits GraphRebuilt; the status mirrors the tension hint.
                app.layout_mode = match app.layout_mode {
                    crate::config::LayoutMode::Column => crate::config::LayoutMode::Force,
                    crate::config::LayoutMode::Force => crate::config::LayoutMode::Column,
                };
                app.status_message = match app.layout_mode {
                    crate::config::LayoutMode::Column => String::from("Layout: column"),
                    crate::config::LayoutMode::Force => String::from("Layout: force"),
                };
                if app.graph.is_some() {
                    app.rebuild_graph();
                }
                return false;
            }
            KeyCode::Char('+') | KeyCode::Char('-') => {
                // Zoom family (change `tiled-window-manager`, 4.2): plain
                // scales the focused pane (graph camera zoom when the graph
                // slot is focused), `Shift` scales the other pane. The
                // camera re-seeds a fit on open; pan/zoom both re-emit the
                // image on the next draw (drag/hover/x/e are unchanged).
                let plus = matches!(key.code, KeyCode::Char('+'));
                let step = if plus { 1 } else { -1 };
                if key.modifiers.shift == graph_slot_focused(app) {
                    cycle_panel_scale(app, plus);
                } else {
                    app.graph_zoom_preset_step(step);
                }
                return false;
            }
            KeyCode::Char('[') | KeyCode::Char(']') => {
                // `Alt` adjusts cable tension on the focused graph pane (design
                // D9: determinism holds per tension value); plain brackets move
                // the left/right big-pane boundary (spec "Adaptive pane
                // layout"). The optimizer pane's weight slider returns before
                // this branch.
                if key.modifiers.alt {
                    let dir = if matches!(key.code, KeyCode::Char(']')) {
                        1
                    } else {
                        -1
                    };
                    app.adjust_tension(dir);
                    return false;
                }
                let delta = if matches!(key.code, KeyCode::Char('[')) {
                    -0.1
                } else {
                    0.1
                };
                app.status_message = adjust_big_boundary(app, delta);
                return false;
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                // Task 2.3: arrows pan the graph camera on the graph surface
                // (mirrors the physical pan-on-overflow model, but gated to the
                // graph's own camera). When the camera has not been seeded yet
                // (the box-drawing path), this is a no-op so navigation is
                // unchanged.
                let (dx, dy) = match key.code {
                    KeyCode::Left => (-1, 0),
                    KeyCode::Right => (1, 0),
                    KeyCode::Up => (0, -1),
                    KeyCode::Down => (0, 1),
                    _ => (0, 0),
                };
                app.graph_pan_if_overflow(dx, dy);
                return false;
            }
            _ => {}
        }
    }

    // Embedded viewer pane handling, routed only while the source viewer pane
    // holds focus (spec "Keys route to the focused view"). Global keys (`q`,
    // Ctrl+C, `l`) fall through to the shared dispatch below.
    if focused_view(app) == Some(ViewType::SourceViewer) {
        match key.code {
            KeyCode::Char('t') => {
                app.source_view_mode = match app.source_view_mode {
                    SourceViewMode::Raw => SourceViewMode::Prettified,
                    SourceViewMode::Prettified => SourceViewMode::Raw,
                };
                return false;
            }
            KeyCode::Char('[') | KeyCode::Char(']') => {
                // Plain brackets move the left/right big-pane boundary; the
                // source viewer does not own them (spec "Adaptive pane
                // layout").
                let delta = if matches!(key.code, KeyCode::Char('[')) {
                    -0.1
                } else {
                    0.1
                };
                app.status_message = adjust_big_boundary(app, delta);
                return false;
            }
            // Pause toggle stays live when source focused (global q/l level).
            KeyCode::Char('p') => {
                app.toggle_processing_pause();
                return false;
            }
            KeyCode::Char('j') => {
                app.source_scroll = app.source_scroll.saturating_add(1);
                return false;
            }
            KeyCode::Char('k') => {
                app.source_scroll = app.source_scroll.saturating_sub(1);
                return false;
            }
            KeyCode::Down => {
                if app.selected_component.is_some() {
                    let next = app.occurrence_cursor.saturating_add(1);
                    app.jump_to_occurrence(next);
                }
                return false;
            }
            KeyCode::Up => {
                if app.selected_component.is_some() {
                    let prev = app.occurrence_cursor.saturating_sub(1);
                    app.jump_to_occurrence(prev);
                }
                return false;
            }
            KeyCode::Home => {
                if app.selected_component.is_some() {
                    app.jump_to_occurrence(0);
                }
                return false;
            }
            KeyCode::End => {
                if let Some(token) = app.selected_component.clone() {
                    if let Some(patch) = app.patch.as_ref() {
                        if let Some(spans) = patch.occurrence_index.get(&token) {
                            if !spans.is_empty() {
                                app.jump_to_occurrence(spans.len() - 1);
                            }
                        }
                    }
                }
                return false;
            }
            _ => {}
        }
    }

    // Label edit overlay entry (`e` on focused datum): overlay > picker > prefix > graph > source > panels.
    // Priority: graph hovered node -> viewer source header -> panel hovered token.
    if matches!(key.code, KeyCode::Char('e')) && key.modifiers.is_none() && app.editing.is_none() {
        // Graph surface takes precedence when it holds focus.
        if graph_slot_focused(app) {
            if let Some(idx) = app.hovered_graph_node {
                if let Some(node) = app.graph.as_ref().and_then(|g| g.nodes.get(idx)).cloned() {
                    let draft = app
                        .current_circuit_store()
                        .get(&node.id)
                        .cloned()
                        .unwrap_or_default();
                    app.editing = Some(crate::app::EditState::new_circuit(node.id.clone(), draft));
                    let settings = crate::config::load(
                        &crate::theme::canonical_theme_name,
                        crate::theme::THEMES,
                    );
                    if let Some(line) = app.editing_status_line(
                        settings.labels.layers_enabled,
                        settings.labels.max_shift_layer,
                    ) {
                        app.status_message = line;
                    } else {
                        app.status_message =
                            format!("Editing circuit {}:{}", node.id.name(), node.id.instance());
                    }
                    return false;
                }
            }
        }
        // Source header (viewer) — resolve to section instance at source focus.
        if focused_view(app) == Some(ViewType::SourceViewer) {
            if let Some(patch) = app.patch.as_ref() {
                // Use selected component's section or fallback to first section.
                let target_idx = app
                    .selected_component
                    .as_ref()
                    .and_then(|tok| patch.occurrence_index.get(tok))
                    .and_then(|spans| spans.first())
                    .map(|s| s.line)
                    .unwrap_or(0);
                // Map line to section index via sections' spans - approximate: pick section containing target line.
                let mut counts: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for section in patch.sections.iter() {
                    let entry = counts.entry(section.name.clone()).or_insert(0);
                    let nid = NodeId::circuit(&section.name, *entry);
                    // First section as fallback when no better mapping.
                    if target_idx == 0 {
                        let draft = app
                            .current_circuit_store()
                            .get(&nid)
                            .cloned()
                            .unwrap_or_default();
                        app.editing = Some(crate::app::EditState::new_circuit(nid.clone(), draft));
                        let settings = crate::config::load(
                            &crate::theme::canonical_theme_name,
                            crate::theme::THEMES,
                        );
                        if let Some(line) = app.editing_status_line(
                            settings.labels.layers_enabled,
                            settings.labels.max_shift_layer,
                        ) {
                            app.status_message = line;
                        } else {
                            app.status_message =
                                format!("Editing circuit {}:{}", nid.name(), nid.instance());
                        }
                        return false;
                    }
                    *entry += 1;
                }
            }
        }
        // Panel hovered token (fallback) - requires patch and hover.
        if let Some(idx) = app.hovered_component {
            if let Some(patch) = app.patch.as_ref() {
                if let Some(comp) = patch.hw_components.get(idx) {
                    let token = comp.id.clone();
                    let settings = crate::config::load(
                        &crate::theme::canonical_theme_name,
                        crate::theme::THEMES,
                    );
                    let max = settings.labels.max_shift_layer.clamp(1, 8);
                    let raw_layer = match app.active_shift {
                        Some(ShiftGroup::Group1) => 1,
                        Some(ShiftGroup::Group2) => 2,
                        Some(ShiftGroup::Group3) => 3,
                        Some(ShiftGroup::Group4) => 4,
                        None => 1,
                    };
                    let layer = if settings.labels.layers_enabled {
                        raw_layer.clamp(1, max)
                    } else {
                        1
                    };
                    let draft = app
                        .current_patch_path
                        .as_ref()
                        .and_then(|p| app.label_store.hw_label(p, &token, layer))
                        .unwrap_or_default();
                    app.editing = Some(crate::app::EditState::new_hw(token.clone(), layer, draft));
                    if let Some(line) = app.editing_status_line(
                        settings.labels.layers_enabled,
                        settings.labels.max_shift_layer,
                    ) {
                        app.status_message = line;
                    } else {
                        app.status_message = format!("Editing {} / Group{}", token, layer);
                    }
                    return false;
                }
            }
        }
    }

    match key.code {
        KeyCode::Char('q') => true,
        KeyCode::Char('c') if key.modifiers.ctrl => true,
        KeyCode::Char('l') => {
            // Opens the picker whether or not a patch is already loaded,
            // so a loaded patch can be swapped for a different one.
            app.showing_picker = true;
            app.picker_dir = std::env::current_dir().unwrap_or_default();
            app.picker_index = 0;
            app.refresh_picker_entries();
            false
        }
        KeyCode::Char('p') => {
            app.toggle_processing_pause();
            false
        }
        KeyCode::Char('r') => {
            // Carousel key: rotate the focused pane's view through the views
            // of its window class (graph/module UI/physical for big panes,
            // source viewer/optimizer for small). No-op on an empty pane and
            // for the optimizer pane, which owns `r` (restore) above.
            app.cycle_view_in_slot(true);
            sync_viewer_focus_from_tiles(app);
            false
        }
        KeyCode::Char('s') => {
            // `s` opens/focuses the Physical tile slot (design D12). While
            // the Physical slot holds focus, `s` toggles the skeleton
            // presentation inside the slot — the physical surface's own
            // presentation switch (mirroring how `p` acts on the graph
            // surface). Free in the normal-key path — the optimizer
            // overlay's `s` (export) returns earlier.
            if physical_slot_focused(app) {
                app.physical_show_skeleton = !app.physical_show_skeleton;
                app.status_message = if app.physical_show_skeleton {
                    String::from("Skeleton: on")
                } else {
                    String::from("Skeleton: off")
                };
            } else {
                app.open_view(ViewType::Physical);
                sync_viewer_focus_from_tiles(app);
            }
            false
        }
        KeyCode::Char('g') => {
            // Enter prefix mode; a repeated `g` re-arms the timer via the
            // cancel-and-fall-through path above.
            app.prefix = Some(PrefixState {
                started: Instant::now(),
            });
            false
        }
        KeyCode::Char('d') if key.modifiers.is_none() => {
            if app.diff_report.is_some() {
                app.toggle_diff_showing();
                if app.diff_showing {
                    app.diff_scope = app.selected_component.clone();
                    if let Some(scoped) = app.status_for_scope() {
                        app.status_message = scoped;
                    } else if let Some(report) = &app.diff_report {
                        app.status_message = format!(
                            "Diff shown: +{} -{} ~{} cables, +{} -{} ~{} nodes",
                            report.added_cables.len(),
                            report.removed_cables.len(),
                            report.changed_cables.len(),
                            report.added_nodes.len(),
                            report.removed_nodes.len(),
                            report.changed_nodes.len()
                        );
                    } else {
                        app.status_message = String::from("Diff shown");
                    }
                } else {
                    app.status_message = String::from("Diff hidden");
                }
            }
            false
        }
        KeyCode::Char('1') => {
            app.active_shift = Some(ShiftGroup::Group1);
            app.status_message = String::from("Shift 1 active");
            false
        }
        KeyCode::Char('2') => {
            app.active_shift = Some(ShiftGroup::Group2);
            app.status_message = String::from("Shift 2 active");
            false
        }
        KeyCode::Char('3') => {
            app.active_shift = Some(ShiftGroup::Group3);
            app.status_message = String::from("Shift 3 active");
            false
        }
        KeyCode::Char('4') => {
            app.active_shift = Some(ShiftGroup::Group4);
            app.status_message = String::from("Shift 4 active");
            false
        }
        KeyCode::Char('m') => {
            // Modifier latch alias (design D): `m` toggles the single-var
            // latch for the hovered component, falling back to the selected
            // one. The MOD status reports the union's counts.
            let token = app
                .hovered_component
                .and_then(|idx| {
                    app.patch
                        .as_ref()
                        .and_then(|p| p.hw_components.get(idx))
                        .map(|c| c.id.clone())
                })
                .or_else(|| app.selected_component.clone());
            match token {
                Some(tok) => {
                    let on = app.toggle_modifier_latch(&tok);
                    app.status_message = if on {
                        app.modifier_status()
                            .unwrap_or_else(|| format!("MOD {} latched", tok))
                    } else {
                        String::from("Modifier latch cleared")
                    };
                }
                None => {
                    app.status_message = String::from("No component to latch");
                }
            }
            false
        }
        KeyCode::Esc => {
            // Unreachable: the focus-routing Esc above handles every Esc that
            // reaches this point (the diff/filter/select-menu/prefix/maximize
            // paths all return earlier). Kept as a safe no-op fallback.
            false
        }
        KeyCode::Char('[') | KeyCode::Char(']') => {
            // Boundary keys (spec "Adaptive pane layout"): `Alt+[`/`Alt+]`
            // move the small-pane boundary, plain `[`/`]` the left/right big
            // boundary. The optimizer (weight) and graph (tension) panes own
            // their `[`/`]` variants and returned before this arm.
            let delta = if matches!(key.code, KeyCode::Char('[')) {
                -0.1
            } else {
                0.1
            };
            app.status_message = if key.modifiers.alt {
                adjust_small_boundary(app, delta)
            } else {
                adjust_big_boundary(app, delta)
            };
            false
        }
        KeyCode::Char('+') | KeyCode::Char('-') => {
            // Zoom family (change `tiled-window-manager`, 4.2): plain scales
            // the panels pane; `Shift` would scale the other pane, which the
            // graph arm above already owns while the graph is open.
            if key.modifiers.shift {
                return false;
            }
            cycle_panel_scale(app, matches!(key.code, KeyCode::Char('+')));
            false
        }
        KeyCode::Char('\\') => {
            // Left-pane vertical split toggle (D3).
            app.toggle_left_split();
            app.status_message = if app.left_split_active {
                String::from("Left split on")
            } else {
                String::from("Left split off")
            };
            false
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if let Some(idx) = app.hovered_component {
                // Capture token id before mutating patch to avoid borrow conflict.
                let token_id = app
                    .patch
                    .as_ref()
                    .and_then(|p| p.hw_components.get(idx))
                    .map(|c| c.id.clone());
                if let Some(token) = token_id {
                    if !app.processing_paused {
                        if let Some(patch) = &mut app.patch {
                            if let Some(comp) = patch.hw_components.get_mut(idx) {
                                toggle_component(comp);
                                app.status_message = format!("Toggled: {}", comp.label);
                            }
                        }
                    }
                    // Commit interaction: toggle AND select. Selection jumps
                    // source_scroll to first occurrence via App::select_component.
                    // Jump happens even while viewer is closed so reopen lands
                    // at the correct line (initial-position rule reapplies).
                    // While paused the toggle is skipped but selection still works.
                    app.select_component(token);
                }
            }
            false
        }
        KeyCode::Up => {
            // Physical-view pan: arrows pan the rack only while the physical
            // pane holds focus (spec "Keys route to the focused view"), and
            // fall back to panel navigation when the rack fits. j/k always
            // navigate.
            if physical_slot_focused(app) && app.physical_pan_if_overflow(0, -1) {
                return false;
            }
            navigate(app, -1);
            false
        }
        KeyCode::Down => {
            if physical_slot_focused(app) && app.physical_pan_if_overflow(0, 1) {
                return false;
            }
            navigate(app, 1);
            false
        }
        KeyCode::Left => {
            if physical_slot_focused(app) {
                app.physical_pan_if_overflow(-1, 0);
            }
            false
        }
        KeyCode::Right => {
            if physical_slot_focused(app) {
                app.physical_pan_if_overflow(1, 0);
            }
            false
        }
        KeyCode::Char('k') => {
            navigate(app, -1);
            false
        }
        KeyCode::Char('j') => {
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
    if app.editing.is_some() {
        return;
    }
    // Help modal click-outside close (design D4): a left-button Down outside
    // the renderer-published modal rect closes help over any surface. Checked
    // before the picker/graph branches so it works even when help
    // overlays them (a click over the picker or graph closes help instead of
    // selecting a file or dragging a node).
    if app.showing_help {
        let inside = app
            .help_modal_rect
            .is_some_and(|rect| rect_contains(&rect, mouse.column, mouse.row));
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) && !inside {
            app.close_help();
            return;
        }
    }
    if app.showing_picker {
        return;
    }
    // Class-layout mouse routing: the graph is a pane, not a full-screen
    // surface, so its mouse handling applies only while the pointer is inside
    // the graph pane's published hit rect. A left-click inside the pane
    // focuses it (spec "Focus routing across panes": a mouse click inside a
    // pane focuses that pane); events elsewhere fall through to the
    // panel/source routing below, and the pointer leaving the graph pane
    // clears node hover so the graph cannot keep a stale hover highlight.
    let graph_pane = app.pane_hit_rects.iter().find(|(id, rect)| {
        app.layout.pane(*id).view == Some(ViewType::Graph)
            && rect_contains(rect, mouse.column, mouse.row)
    });
    if let Some(_graph_pane) = graph_pane {
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            app.open_view(ViewType::Graph);
            sync_viewer_focus_from_tiles(app);
        }
        handle_graph_mouse(mouse, app);
        return;
    }
    app.hovered_graph_node = None;
    // Minimap click-to-scroll: uses renderer-published minimap geometry with
    // the same proportional mapping as the viewport indicator in ui.rs
    // (indicator: scroll * inner_h / total_lines). Click must work whenever
    // the embedded viewer is visible, regardless of focus, and takes
    // precedence over panel interactions (picker already returned above).
    if app.showing_viewer {
        if let Some(rect) = app.minimap_rect {
            if rect_contains(&rect, mouse.column, mouse.row)
                && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            {
                if let Some(patch) = app.patch.as_ref() {
                    let total_lines = if !patch.raw_lines.is_empty() {
                        patch.raw_lines.len()
                    } else {
                        patch.sections.len().max(1)
                    };
                    // Mirror ui.rs render_minimap: inner area excludes the
                    // 1-cell border on each side; viewport_h proxy is inner_h.
                    let inner_h = rect.height.saturating_sub(2) as usize;
                    if inner_h != 0 {
                        let inner_y = rect.y.saturating_add(1);
                        // Map click y to inner row, clamping border clicks to
                        // the nearest inner row so the fraction stays 0..1.
                        let row = if mouse.row < inner_y {
                            0
                        } else if mouse.row >= inner_y + inner_h as u16 {
                            inner_h.saturating_sub(1)
                        } else {
                            (mouse.row - inner_y) as usize
                        };
                        // Invert indicator mapping: row = scroll * inner_h / total
                        // -> scroll = row * total / inner_h (top-aligned).
                        let raw_target = row * total_lines / inner_h;
                        // Center the clicked line in the viewport, matching the
                        // requirement's "minus viewport half" and keeping the
                        // handler/ui mapping consistent so the indicator tracks.
                        let viewport_h = inner_h;
                        let centered = raw_target.saturating_sub(viewport_h / 2);
                        let max_scroll = total_lines.saturating_sub(viewport_h);
                        app.source_scroll = centered.min(max_scroll);
                    }
                }
                return;
            }
        }
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
            // A click inside a pane focuses that pane (spec "Focus routing
            // across panes"); an empty pane has no view to focus.
            focus_pane_at(app, mouse.column, mouse.row);
            sync_viewer_focus_from_tiles(app);
            if let Some(idx) = hit {
                app.hovered_component = Some(idx);
                let token_id = app
                    .patch
                    .as_ref()
                    .and_then(|p| p.hw_components.get(idx))
                    .map(|c| c.id.clone());
                if let Some(token) = token_id {
                    if !app.processing_paused {
                        if let Some(patch) = &mut app.patch {
                            if let Some(comp) = patch.hw_components.get_mut(idx) {
                                toggle_component(comp);
                                app.status_message = format!("Toggled: {}", comp.label);
                            }
                        }
                    }
                    // Modifier hold: mouse Down without keyboard modifiers on a component.
                    // Sets `hold_component` so the panels paint a wash backdrop; clears
                    // when the mouse is released or leaves the panel area. Ctrl+Click /
                    // Ctrl+Shift+Click toggle the persistent single-var latch instead
                    // (design D chord); the momentary hold stays on the plain Down.
                    if mouse.modifiers.ctrl {
                        let on = app.toggle_modifier_latch(&token);
                        app.status_message = if on {
                            app.modifier_status()
                                .unwrap_or_else(|| format!("MOD {} latched", token))
                        } else {
                            String::from("Modifier latch cleared")
                        };
                    } else if mouse.modifiers == key_modifiers::NONE {
                        app.hold_component = Some(token.clone());
                        app.refresh_modifier_influence();
                        if let Some(status) = app.modifier_status() {
                            app.status_message = status;
                        }
                    }
                    app.select_component(token);
                }
            } else {
                // Empty-space click: clear selection without moving
                // source_scroll (deselection stability). Ignore clicks on the
                // minimap column (click-to-scroll owns it) and on the source
                // viewer pane (a bare source click keeps the selection so
                // occurrence navigation keeps working there).
                let on_minimap = app
                    .minimap_rect
                    .is_some_and(|rect| rect_contains(&rect, mouse.column, mouse.row));
                let over_viewer_pane = app.pane_hit_rects.iter().any(|(id, rect)| {
                    app.layout.pane(*id).view == Some(ViewType::SourceViewer)
                        && rect_contains(rect, mouse.column, mouse.row)
                });
                if !on_minimap && !over_viewer_pane {
                    app.clear_selected_component();
                }
            }
        }
        MouseEventKind::Up(_) => {
            // Momentary hold release: drop the held token and its wash. The
            // status clears only when no latch survives; otherwise it
            // falls back to the latched MOD line.
            if app.hold_component.is_some() {
                app.hold_component = None;
                app.refresh_modifier_influence();
                if app.latched_component.is_none() {
                    app.status_message.clear();
                } else if let Some(status) = app.modifier_status() {
                    app.status_message = status;
                }
            }
        }
        MouseEventKind::ScrollUp => {
            // Physical-view wheel-pan (4.3): when the rack overflows
            // vertically the wheel pans instead of adjusting the hovered
            // knob/fader value (design D5; `physical_pan_if_overflow` also
            // gates viewer/graph surfaces away).
            let panned = app.physical_pan_if_overflow(0, -1);
            if !panned {
                if let Some(idx) = hit {
                    if !app.processing_paused {
                        if let Some(patch) = &mut app.patch {
                            if let Some(comp) = patch.hw_components.get_mut(idx) {
                                adjust_value(comp, 0.05);
                            }
                        }
                    }
                }
            }
        }
        MouseEventKind::ScrollDown => {
            let panned = app.physical_pan_if_overflow(0, 1);
            if !panned {
                if let Some(idx) = hit {
                    if !app.processing_paused {
                        if let Some(patch) = &mut app.patch {
                            if let Some(comp) = patch.hw_components.get_mut(idx) {
                                adjust_value(comp, -0.05);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Handle mouse input while the graph surface is open. The graph has no hover
/// or panel concept: a left-button Down on a renderer-published node rect
/// begins a drag, Drag follows the pointer with a damped local re-settle and
/// a `NodeMoved` event, and Up releases. Everything else is a no-op (design
/// D1/D7). No other app state is touched during a drag.
fn handle_graph_mouse(mouse: MouseEvent, app: &mut App) {
    match mouse.kind {
        MouseEventKind::Moved => {
            let hit = app
                .graph_node_rects
                .iter()
                .find(|(_, rect)| rect_contains(rect, mouse.column, mouse.row))
                .map(|(idx, _)| *idx);
            app.hovered_graph_node = hit;
            // Back-edge sink hover (design D2): surface "reads _X 1 loop
            // behind" in the status bar; non-back-edge hovers leave the
            // previous status untouched.
            if let Some(idx) = hit {
                if let Some(text) = app.back_edge_hover_status(idx) {
                    app.status_message = text;
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Check for minimap click first
            if let Some(minimap_rect) = app.graph_minimap_rect {
                if rect_contains(&minimap_rect, mouse.column, mouse.row) {
                    // Minimap click: pan camera to clicked world position.
                    // `graph_minimap_rect` is the outer panel in cell space;
                    // `(ix, iy)` is the inner-panel origin in canvas space, so
                    // invert `minimap_layout`'s `map` directly (no hardcoded
                    // inset): world = bx + (click - ix) / sx.
                    if let Some(transform) = app.graph_minimap_transform {
                        let (bx, by, _bw, _bh, ix, iy, sx, sy) = transform;
                        if sx > 0.0 && sy > 0.0 {
                            let world_x = bx + (mouse.column as f32 - ix) / sx;
                            let world_y = by + (mouse.row as f32 - iy) / sy;
                            // Pan camera to center on world position
                            if let Some(mut camera) = app.graph_camera {
                                let (vw, vh) = app.graph_canvas_px.unwrap_or((800.0, 600.0));
                                camera.pan = (
                                    world_x * camera.zoom - vw / 2.0,
                                    world_y * camera.zoom - vh / 2.0,
                                );
                                app.graph_camera = Some(camera);
                                app.status_message = format!(
                                    "Minimap click: pan to ({:.0}, {:.0})",
                                    world_x, world_y
                                );
                            }
                        }
                    }
                    return;
                }
            }
            let hit = app
                .graph_node_rects
                .iter()
                .find(|(_, rect)| rect_contains(rect, mouse.column, mouse.row))
                .map(|(idx, _)| *idx);
            let Some(node_index) = hit else {
                app.hovered_graph_node = None;
                return;
            };
            app.hovered_graph_node = Some(node_index);
            // Clicking a node selects its circuit (design D4): shared selection
            // drives the source-viewer jump, panel hardware highlight, and the
            // terminal-tile node highlight. Mirrors the window's press-select.
            if let Some(node) = app.graph.as_ref().and_then(|g| g.nodes.get(node_index)) {
                app.select_circuit(node.id.clone());
            }
            // Record the grab offset so the node follows the pointer without
            // jumping to the grab point on the first drag delta.
            if let Some((px, py)) = app.graph_positions.get(node_index).copied() {
                app.graph_drag = Some(GraphDrag {
                    node_index,
                    offset_x: px - mouse.column as f32,
                    offset_y: py - mouse.row as f32,
                });
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let Some(drag) = app.graph_drag.as_ref() else {
                return;
            };
            let Some(graph) = app.graph.as_ref() else {
                return;
            };
            let Some(node_id) = graph.nodes.get(drag.node_index).map(|n| n.id.clone()) else {
                return;
            };
            if let Some(pos) = app.graph_positions.get_mut(drag.node_index) {
                *pos = (
                    clamp_drag(mouse.column as f32 + drag.offset_x),
                    clamp_drag(mouse.row as f32 + drag.offset_y),
                );
            }
            let pins = app.pinned_indices(graph);
            layout::local_resettle(
                graph,
                &mut app.graph_positions,
                &node_id,
                layout::LOCAL_RADIUS,
                layout::LOCAL_ITERATIONS,
                &pins,
                app.tension,
            );
            app.notify_node_moved(&node_id);
        }
        MouseEventKind::Up(_) => {
            // Task 3.1 (design D7): the drag result becomes a fixed anchor —
            // the dropped node is auto-pinned so it stays where placed
            // instead of snapping back to spring equilibrium on a rebuild.
            if let Some(drag) = app.graph_drag.take() {
                if let Some(node) = app
                    .graph
                    .as_ref()
                    .and_then(|g| g.nodes.get(drag.node_index))
                {
                    app.pinned.insert(node.id.clone());
                }
            }
            // Modifier hold: release the mouse hold wash. The status
            // clears only when no latch survives; otherwise it falls
            // back to the latched MOD line.
            if app.hold_component.is_some() {
                app.hold_component = None;
                app.refresh_modifier_influence();
                if app.latched_component.is_none() {
                    app.status_message.clear();
                } else if let Some(status) = app.modifier_status() {
                    app.status_message = status;
                }
            }
        }
        MouseEventKind::ScrollUp => {
            // Task 2.3: wheel pans the graph camera on the graph surface
            // when it overflows; otherwise it is a no-op (the box-drawing
            // path has no camera, so `graph_pan_if_overflow` returns false).
            app.graph_pan_if_overflow(0, -1);
        }
        MouseEventKind::ScrollDown => {
            app.graph_pan_if_overflow(0, 1);
        }
        _ => {}
    }
}

/// Bound a dragged node's position to a sane virtual-plane window. Mouse
/// coordinates are already inside the terminal, but the grab offset can carry
/// the sum far out; the renderer's min/max fit maps any bounded set onto the
/// surface, so this only guards against float blowup while keeping the node
/// reachable.
const DRAG_POSITION_LIMIT: f32 = 10_000.0;
fn clamp_drag(v: f32) -> f32 {
    v.clamp(-DRAG_POSITION_LIMIT, DRAG_POSITION_LIMIT)
}

/// `x` on the hovered graph node: toggle per-circuit processing and rebuild
/// the graph (terminal graph key; shared with the GPU window, task 3.1).
/// No-op without a hovered node; the caller has already consumed the key.
fn graph_node_processing_toggle(app: &mut App, idx: usize) {
    let Some(node) = app.graph.as_ref().and_then(|g| g.nodes.get(idx)).cloned() else {
        return;
    };
    let now_disabled = app.toggle_circuit_processing(&node.circuit, node.instance_index);
    app.rebuild_graph();
    app.status_message = if now_disabled {
        format!(
            "Processing disabled: {} {}",
            node.circuit, node.instance_index
        )
    } else {
        format!(
            "Processing enabled: {} {}",
            node.circuit, node.instance_index
        )
    };
}

/// `p` on the hovered graph node: toggle the pin anchor and rebuild the graph
/// (terminal graph key; shared with the GPU window, task 3.1). No-op without
/// a hovered node.
fn graph_node_pin_toggle(app: &mut App, idx: usize) {
    let Some(node) = app.graph.as_ref().and_then(|g| g.nodes.get(idx)).cloned() else {
        return;
    };
    let now_pinned = app.toggle_pin(&node.id);
    app.rebuild_graph();
    app.status_message = if now_pinned {
        format!("Pinned: {} {}", node.circuit, node.instance_index)
    } else {
        format!("Unpinned: {} {}", node.circuit, node.instance_index)
    };
}

/// `e` on the hovered graph node: open the circuit label edit overlay with the
/// stored label as draft (the terminal graph `e` branch, extracted so the GPU
/// window shares it, task 3.1). Returns whether an edit was started.
fn begin_graph_node_edit(app: &mut App, idx: usize) -> bool {
    let Some(node) = app.graph.as_ref().and_then(|g| g.nodes.get(idx)).cloned() else {
        return false;
    };
    let draft = app
        .current_patch_path
        .as_ref()
        .and_then(|p| app.label_store.circuit_label(p, &node.id))
        .unwrap_or_default();
    app.editing = Some(crate::app::EditState::new_circuit(node.id.clone(), draft));
    let settings = crate::config::load(&crate::theme::canonical_theme_name, crate::theme::THEMES);
    if let Some(line) = app.editing_status_line(
        settings.labels.layers_enabled,
        settings.labels.max_shift_layer,
    ) {
        app.status_message = line;
    } else {
        app.status_message = format!("Editing circuit {}:{}", node.id.name(), node.id.instance());
    }
    true
}

/// Apply one GPU-graph-window frame to the shared `App` (task 3.1).
///
/// The window is another pointer/keyboard surface over the same graph: hover
/// and drag hit-test window-space pointers against the graph layout via
/// [`crate::graph_render::GraphCamera`] (`pixel → world → node index`),
/// mirroring how `graph_node_rects` drives the terminal surface. `x`/`p`/`e`
/// act on the hovered node exactly like the terminal graph keys. No camera
/// yet (graph not fitted) means nothing is interactive. The windowed loop in
/// `main.rs` owns both the window and the `App`, so it calls this once per
/// painted frame (D6: the loop acts on what it owns).
pub fn handle_graph_window_frame(frame: &crate::gui::WindowFrame, app: &mut App) {
    let Some(mut camera) = app.graph_camera else {
        app.hovered_graph_node = None;
        return;
    };
    // The scene is painted as absolute window coordinates from the graph pane's
    // origin (the egui painter has no translate; the scene builder shifts the
    // camera by the pane origin), but this shared camera is pane-relative. The
    // pointer and wheel-zoom anchor arrive in window coordinates, so map them
    // into the pane before the world mapping, or click/drag/wheel land one
    // pane origin away from the drawn node.
    let (ox, oy) = graph_pane_origin(app);
    // Window pan/zoom reaches the shared camera (design D5): both surfaces
    // consume the same camera, so a middle-drag or wheel zoom in the window
    // moves the terminal tile identically. Applied before hit-testing so the
    // hover/drag hit rects stay aligned with the freshly painted view.
    if frame.pan_delta != (0.0, 0.0) {
        camera = crate::gui::camera_pan(&camera, frame.pan_delta.0, frame.pan_delta.1);
    }
    if let Some((factor, (ax, ay))) = frame.zoom {
        camera = crate::gui::camera_zoom_about(&camera, factor, (ax - ox, ay - oy));
    }
    app.graph_camera = Some(camera);
    // Hit-test against each node's world-space extent (graph-zoom-node-scaling,
    // design decision 5): the drawn frame *is* the node's world rect projected
    // through the camera, so testing in world units is exact at every zoom and
    // needs no `/ zoom` conversion. `node_world_sizes` gives each node its own
    // world `(width, height)` (node positions are the box's top-left corner).
    // A minimum hit size keeps a node selectable when its drawn frame is only a
    // few pixels, expanding the world test rect symmetrically. First match wins.
    let world = app
        .graph
        .as_ref()
        .map(crate::layout::node_world_sizes)
        .unwrap_or_default();
    let min_w = App::GRAPH_MIN_HIT_PX / camera.zoom;
    let min_h = App::GRAPH_MIN_HIT_PX / camera.zoom;
    let hit = frame.pointer.and_then(|(px, py)| {
        let (wx, wy) = camera.pixel_to_world(px - ox, py - oy);
        app.graph_positions
            .iter()
            .enumerate()
            .find_map(|(i, &(x, y))| {
                let (w, h) = world.get(i).copied().unwrap_or((0.0, 0.0));
                let hw = (w.max(min_w) - w) / 2.0;
                let hh = (h.max(min_h) - h) / 2.0;
                (wx >= x - hw && wx < x + w + hw && wy >= y - hh && wy < y + h + hh).then_some(i)
            })
    });
    app.hovered_graph_node = if frame.pointer.is_some() { hit } else { None };

    // Marquee commit (ibu-marquee-selection): the window already highlighted
    // `marquee.nodes` in scene order, so the first enclosed index is the
    // primary. It arrives in the same window-space mapping the click path
    // uses (pane origin + pixel_to_world), matching the drawn highlight.
    // Empty marquee is a no-op. The guard keeps a held drag from churning
    // scroll/cursor via repeated `select_circuit` calls.
    if let Some(marquee) = frame.marquee.as_ref() {
        if let Some(&first) = marquee.nodes.first() {
            if let Some(node_id) = app
                .graph
                .as_ref()
                .and_then(|g| g.nodes.get(first))
                .map(|n| n.id.clone())
            {
                if app.selected_circuit() != Some(&node_id) {
                    app.select_circuit(node_id);
                }
            }
        }
    }

    if frame.primary_pressed {
        // Check for minimap click first
        if let (Some(pointer), Some(panel_egui)) = (frame.pointer, app.graph_minimap_panel_egui) {
            // panel_egui is canvas-relative; graph pane origin is (ox, oy)
            let panel_window = egui::Rect::from_min_size(
                egui::pos2(panel_egui.min.x + ox, panel_egui.min.y + oy),
                panel_egui.size(),
            );
            if panel_window.contains(egui::pos2(pointer.0, pointer.1)) {
                // Minimap click: pan camera to clicked world position
                if let Some(transform) = app.graph_minimap_transform {
                    let (bx, by, _bw, _bh, ix, iy, sx, sy) = transform;
                    // `(ix, iy)` is the inner-panel origin in canvas space (the
                    // outer panel inset by MINIMAP_INSET). Shift it by the pane
                    // origin to match the window-space pointer, then invert
                    // `minimap_layout`'s `map`: world = bx + (rel - ix) / sx.
                    // Guard the scale factors: a zero extent would divide by 0.
                    if sx > 0.0 && sy > 0.0 {
                        let rel_x = pointer.0 - (ix + ox);
                        let rel_y = pointer.1 - (iy + oy);
                        let world_x = bx + rel_x / sx;
                        let world_y = by + rel_y / sy;
                        // Pan camera to center on world position
                        if let Some(mut camera) = app.graph_camera {
                            let (vw, vh) = app.graph_canvas_px.unwrap_or((800.0, 600.0));
                            camera.pan = (
                                world_x * camera.zoom - vw / 2.0,
                                world_y * camera.zoom - vh / 2.0,
                            );
                            app.graph_camera = Some(camera);
                            app.status_message =
                                format!("Minimap click: pan to ({:.0}, {:.0})", world_x, world_y);
                        }
                    }
                }
                return;
            }
        }
        let Some(node_index) = hit else {
            app.hovered_graph_node = None;
            return;
        };
        app.hovered_graph_node = Some(node_index);
        // Clicking a node selects its circuit (design D4) — shared selection
        // propagates to the source viewer, panels, and terminal tile.
        if let Some(node) = app.graph.as_ref().and_then(|g| g.nodes.get(node_index)) {
            app.select_circuit(node.id.clone());
        }
        let Some((px, py)) = frame.pointer else {
            return;
        };
        let (wx, wy) = camera.pixel_to_world(px - ox, py - oy);
        if let Some((nx, ny)) = app.graph_positions.get(node_index).copied() {
            // Grab offset in world units so the node follows the pointer
            // without jumping on the first drag delta (mirrors GraphDrag).
            app.graph_drag = Some(GraphDrag {
                node_index,
                offset_x: nx - wx,
                offset_y: ny - wy,
            });
        }
    }

    // A drag frame moves the node only when the button was already down at
    // the start of this frame: egui reports `primary_pressed` and
    // `primary_down` together on the press frame, and the press frame only
    // grabs the node (mirrors the terminal Down/Drag event split).
    if frame.primary_down && !frame.primary_pressed {
        let Some(drag) = app.graph_drag.as_ref() else {
            return;
        };
        let Some(graph) = app.graph.as_ref() else {
            return;
        };
        let Some(node_id) = graph.nodes.get(drag.node_index).map(|n| n.id.clone()) else {
            return;
        };
        let Some((px, py)) = frame.pointer else {
            return;
        };
        let (wx, wy) = camera.pixel_to_world(px - ox, py - oy);
        if let Some(pos) = app.graph_positions.get_mut(drag.node_index) {
            *pos = (
                clamp_drag(wx + drag.offset_x),
                clamp_drag(wy + drag.offset_y),
            );
        }
        let pins = app.pinned_indices(graph);
        layout::local_resettle(
            graph,
            &mut app.graph_positions,
            &node_id,
            layout::LOCAL_RADIUS,
            layout::LOCAL_ITERATIONS,
            &pins,
            app.tension,
        );
        app.notify_node_moved(&node_id);
    }

    if frame.primary_released {
        // Design D7: the dropped node becomes a fixed anchor so it stays where
        // placed instead of snapping back on a rebuild (mirrors the terminal).
        if let Some(drag) = app.graph_drag.take() {
            if let Some(node) = app
                .graph
                .as_ref()
                .and_then(|g| g.nodes.get(drag.node_index))
            {
                app.pinned.insert(node.id.clone());
            }
        }
    }

    // Design D3: the camera keys act on the pane without a hovered node,
    // mirroring the terminal graph's `c`/`Shift+c`. The fit status text
    // matches the terminal path exactly.
    for key in &frame.keys {
        match key {
            crate::gui::WindowGraphKey::CenterGraph => {
                app.center_graph_camera();
            }
            crate::gui::WindowGraphKey::FitGraph => {
                let viewport = app.graph_canvas_px.unwrap_or((1280.0, 800.0));
                app.fit_graph_camera(viewport);
                app.status_message = format!(
                    "Graph zoom {:.0}%",
                    App::GRAPH_ZOOM_PRESETS[App::GRAPH_ZOOM_FIT_INDEX] * 100.0
                );
            }
            crate::gui::WindowGraphKey::ToggleProcessing
            | crate::gui::WindowGraphKey::TogglePin
            | crate::gui::WindowGraphKey::BeginEdit => {}
        }
    }

    for key in &frame.keys {
        let Some(idx) = app.hovered_graph_node else {
            continue;
        };
        match key {
            crate::gui::WindowGraphKey::ToggleProcessing => graph_node_processing_toggle(app, idx),
            crate::gui::WindowGraphKey::TogglePin => graph_node_pin_toggle(app, idx),
            crate::gui::WindowGraphKey::BeginEdit => {
                begin_graph_node_edit(app, idx);
            }
            crate::gui::WindowGraphKey::CenterGraph | crate::gui::WindowGraphKey::FitGraph => {
                // Handled above: the camera keys act on the pane, not a node.
                continue;
            }
        }
    }
}

/// The min corner (in points) of the graph pane the renderer published for
/// this frame — the origin the full graph scene is painted from. (0, 0) when
/// the graph fills the whole window or no pane rect was published yet (the
/// whole-window canvas origin).
fn graph_pane_origin(app: &App) -> (f32, f32) {
    app.pane_hit_rects
        .iter()
        .find(|(id, _)| app.layout.pane(*id).view == Some(crate::app::ViewType::Graph))
        .map(|(_, rect)| (rect.x as f32, rect.y as f32))
        .unwrap_or((0.0, 0.0))
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
        KeyCode::Esc => {
            app.reset_picker_filter();
            app.showing_picker = false;
            app.diff_picker_active = false;
            false
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.picker_index > 0 {
                app.picker_index -= 1;
            }
            false
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.picker_index < app.picker_entries.len().saturating_sub(1) {
                app.picker_index += 1;
            }
            false
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            // Favourites toggle for the highlighted picker entry (file-picker-favourites 3.1).
            // Directories toggle like files; the pinned section shows both. Only the
            // parent sentinel is excluded.
            if key.modifiers.ctrl {
                // Ctrl+F toggles the latching filter (spec scenario: Toggle
                // filter with Ctrl+f). Second press while active turns it off
                // and clears the string.
                if app.picker_filter_active {
                    app.picker_filter_active = false;
                    app.picker_filter.clear();
                    app.status_message = String::from("Filter: off");
                    app.refresh_picker_entries();
                } else {
                    app.picker_filter_active = true;
                    app.picker_filter.clear();
                    app.status_message = String::from("Filter: on (type to filter)");
                }
                return false;
            }
            if key.modifiers.alt {
                return false;
            }
            if let Some(selected_path) = app.picker_entries.get(app.picker_index).cloned() {
                if is_picker_parent_entry(&selected_path) {
                    return false;
                }
                let target_key = crate::favorites::FavoritesStore::canonical_key(&selected_path);
                let now_favourited = app.favorites.toggle(&selected_path);
                let _ = app.favorites.save();
                let label = selected_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| selected_path.to_string_lossy().to_string());
                app.status_message = if now_favourited {
                    format!("Favourited: {label}")
                } else {
                    format!("Unfavourited: {label}")
                };
                app.refresh_picker_entries();
                if let Some(pos) = app
                    .picker_entries
                    .iter()
                    .position(|p| crate::favorites::FavoritesStore::canonical_key(p) == target_key)
                {
                    app.picker_index = pos;
                } else if app.picker_index >= app.picker_entries.len() {
                    app.picker_index = app.picker_entries.len().saturating_sub(1);
                }
            }
            false
        }
        KeyCode::Char(d @ '0'..='9') => {
            // Digit keys fast-select a pinned favourite slot (0-based in the
            // sorted favourites list). Directories navigate in, files open like
            // Enter; out-of-range digits are silent.
            if key.modifiers.ctrl || key.modifiers.alt {
                return false;
            }
            let slot = d.to_digit(10).unwrap_or(0) as usize;
            let favs = app.picker_entries_with_favourites();
            if let Some(fav) = favs.get(slot) {
                open_picker_entry(app, fav.clone());
            }
            false
        }
        KeyCode::Char('q') if app.picker_filter_active => {
            // q ends the latching filter without quitting: the picker owns
            // keys while open, so this arm is the only q route and must not
            // fall through to the quit path.
            app.reset_picker_filter();
            app.refresh_picker_entries();
            false
        }
        KeyCode::Backspace if app.picker_filter_active => {
            app.picker_filter.pop();
            app.refresh_picker_entries();
            false
        }
        KeyCode::Char(c) if app.picker_filter_active && !c.is_ascii_digit() => {
            // Digits never land here: 0-9 must keep routing to the
            // favourite-slot fast-select above while the filter is latched.
            let previous = app.picker_index;
            app.picker_filter.push(c);
            app.refresh_picker_entries();
            // The narrowing list may have dropped the previously selected
            // entry; refresh clamps to the tail, so re-anchor at the top
            // instead of leaving a stale position.
            if previous >= app.picker_entries.len() {
                app.picker_index = 0;
            }
            false
        }
        KeyCode::Enter => {
            if let Some(selected_path) = app.picker_entries.get(app.picker_index).cloned() {
                if !is_entry_selectable(&selected_path) {
                    return false;
                }
                // Parent ".." sentinel: navigate up to the picker dir's parent.
                // Handled before the metadata `is_dir` branch because a bare
                // ".." path resolves against the process cwd, not picker_dir.
                if is_picker_parent_entry(&selected_path) {
                    if let Some(parent) = app.picker_dir.parent() {
                        app.picker_dir = parent.to_path_buf();
                        app.picker_index = 0;
                        app.refresh_picker_entries();
                    }
                    return false;
                }
                open_picker_entry(app, selected_path);
            }
            false
        }
        _ => false,
    }
}

/// Open a picker entry: directories navigate in-place (picker stays open),
/// `.ini` files load through the diff or patch path and close the picker.
/// Shared by the Enter arm and the digit-slot fast-select arm so both keep
/// identical open semantics.
fn open_picker_entry(app: &mut App, path: PathBuf) {
    let is_dir = path.metadata().is_ok_and(|m| m.is_dir());
    if is_dir {
        app.picker_dir = path;
        app.picker_index = 0;
        app.refresh_picker_entries();
        return;
    }
    if app.diff_picker_active {
        match app.load_diff_patch(&path) {
            Ok(()) => {
                if let Some(scoped) = app.status_for_scope() {
                    app.status_message = scoped;
                } else if let Some(report) = &app.diff_report {
                    app.status_message = format!(
                        "Diff loaded: +{} -{} ~{} cables, +{} -{} ~{} nodes",
                        report.added_cables.len(),
                        report.removed_cables.len(),
                        report.changed_cables.len(),
                        report.added_nodes.len(),
                        report.removed_nodes.len(),
                        report.changed_nodes.len()
                    );
                }
                app.selected_file = Some(path);
                app.showing_picker = false;
                app.diff_picker_active = false;
                app.reset_picker_filter();
            }
            Err(e) => {
                app.status_message = format!("Failed to load diff patch: {}", e);
            }
        }
    } else {
        match Patch::from_ini_file(&path) {
            Ok(patch) => {
                // Route through load_patch_at so picker loads run
                // validation, the Error gate, and LabelStore path
                // keying. The picker must close on both outcomes: a
                // gated load has to surface the validation modal,
                // and the picker outranks it in key priority.
                let _ = app.load_patch_at(&path, patch);
                app.hovered_component = None;
                app.selected_file = Some(path);
                app.showing_picker = false;
                app.diff_picker_active = false;
                app.reset_picker_filter();
            }
            Err(e) => {
                app.status_message = format!("Failed to load patch: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::FocusSlot;
    use crate::app::Rect;
    use crate::events::Event;
    use crate::patch::Patch;
    use std::sync::{Mutex, OnceLock};

    fn fav_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn app_with_fixture() -> App {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        // Place component 0 (B1.1) at (0,0)-(16,2) and component 1 (L1.1) at (16,0)-(32,2).
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2)), (1, Rect::new(16, 0, 16, 2))];
        app
    }

    fn app_with_source_navigation() -> App {
        let patch =
            Patch::from_ini_file(std::path::Path::new("fixtures/source_navigation.ini")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2)), (1, Rect::new(16, 0, 16, 2))];
        app
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: key_modifiers::NONE,
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
    fn momentary_hold_sets_mod_status_and_release_clears_or_keeps_latch() {
        let mut app = app_with_fixture();
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 1),
            &mut app,
        );
        assert_eq!(app.hold_component.as_deref(), Some("B1.1"));
        let held = app.modifier_status().expect("hold reports MOD");
        assert!(held.starts_with("MOD B1.1"), "status: {held}");
        assert_eq!(app.status_message, held);

        handle_mouse_event(mouse(MouseEventKind::Up(MouseButton::Left), 5, 1), &mut app);
        assert!(app.hold_component.is_none());
        assert!(app.status_message.is_empty());

        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 1),
            &mut app,
        );
        app.toggle_modifier_latch("B1.1");
        let latched = app.modifier_status().expect("latch reports MOD");
        handle_mouse_event(mouse(MouseEventKind::Up(MouseButton::Left), 5, 1), &mut app);
        assert!(app.hold_component.is_none());
        assert_eq!(app.status_message, latched);
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

    /// A patch-loaded app whose rack overflows a deterministic 80×24 main
    /// viewport on both axes (4.3 pan tests).
    fn app_with_overflowing_rack() -> App {
        let mut app = app_with_fixture();
        app.physical_rack_size = (200, 100);
        app.physical_viewport = Some(Rect::new(0, 3, 80, 24));
        app
    }

    #[test]
    fn arrow_pan_pans_toward_pressed_direction_when_rack_overflows() {
        let mut app = app_with_overflowing_rack();
        // Physical is the module UI and holds focus at startup (design D1), so
        // arrows already route to its pan — no `s` needed to focus it.
        // Right/Down pan positive (screen content shifts opposite, D5);
        // Left/Up reverse. Panning must not move the keyboard cursor.
        handle_event(key(KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (8.0, 0.0));
        assert_eq!(app.hovered_component, None, "pan must not navigate");
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.physical_offset, (8.0, 8.0));
        handle_event(key(KeyCode::Left), &mut app);
        assert_eq!(app.physical_offset, (0.0, 8.0));
        handle_event(key(KeyCode::Up), &mut app);
        assert_eq!(app.physical_offset, (0.0, 0.0));
        assert_eq!(
            app.status_message,
            "Physical 100% \u{B7} Pan 0/0 \u{B7} Skeleton: off"
        );
    }

    #[test]
    fn arrow_keys_preserve_navigation_when_rack_fits() {
        let mut app = app_with_fixture();
        app.physical_rack_size = (40, 10);
        app.physical_viewport = Some(Rect::new(0, 3, 80, 24));
        // Rack fits the viewport: Down/Up navigate, Left/Right stay no-ops.
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.hovered_component, Some(1));
        handle_event(key(KeyCode::Up), &mut app);
        assert_eq!(app.hovered_component, Some(0));
        handle_event(key(KeyCode::Left), &mut app);
        handle_event(key(KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (0.0, 0.0));
    }

    #[test]
    fn arrow_pan_only_when_the_physical_pane_is_focused() {
        let mut app = app_with_overflowing_rack();
        // Physical IS the module UI and is focused at startup: arrows pan the overflowing rack.
        handle_event(key(KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (8.0, 0.0));
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.physical_offset, (8.0, 8.0));

        // Switch focus to a small pane: arrows should NOT pan (source viewer
        // focused), so the offset stays exactly where the physical pane left it.
        handle_event(key(KeyCode::Tab), &mut app); // -> viewer pane (SmallTop)
        handle_event(key(KeyCode::Right), &mut app);
        assert_eq!(
            app.physical_offset,
            (8.0, 8.0),
            "physical offset should not change when viewer focused"
        );

        // Switch back to Physical: arrows pan again, from the unchanged offset.
        handle_event(key(KeyCode::Tab), &mut app); // -> Physical pane (BigLeft)
        handle_event(key(KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (16.0, 8.0));
    }

    #[test]
    fn wheel_pans_when_rack_overflows_and_adjusts_knob_otherwise() {
        let content = "[pot]\n    pot = P1.1\n    output = _X\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];
        app.physical_rack_size = (200, 100);
        app.physical_viewport = Some(Rect::new(0, 3, 80, 24));

        // Vertical overflow forces panning even over a hovered knob cell;
        // scroll up pans up (negative offset) and leaves the value alone.
        handle_mouse_event(mouse(MouseEventKind::ScrollUp, 5, 1), &mut app);
        assert_eq!(app.physical_offset, (0.0, -8.0));
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => assert!(v.abs() < 1e-6, "pan must not adjust"),
            _ => panic!("expected Value state"),
        }
        handle_mouse_event(mouse(MouseEventKind::ScrollDown, 5, 1), &mut app);
        assert_eq!(app.physical_offset, (0.0, 0.0));

        // Rack fits: the wheel adjusts the hovered knob as before.
        app.physical_rack_size = (40, 10);
        handle_mouse_event(mouse(MouseEventKind::ScrollUp, 5, 1), &mut app);
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => assert!((v - 0.05).abs() < 1e-6),
            _ => panic!("expected Value state"),
        }
    }

    #[test]
    fn zoom_shows_physical_status_hint_with_patch_loaded() {
        let mut app = app_with_fixture();
        // `+` climbs one preset (100% → 150%) and folds the zoom into the
        // physical segment instead of the legacy "Scaling: N%" message.
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 1.5);
        assert_eq!(app.physical_zoom, 1.5);
        assert_eq!(
            app.status_message,
            "Physical 150% \u{B7} Pan 0/0 \u{B7} Skeleton: off"
        );
    }

    #[test]
    fn mouse_ignored_while_picker_open() {
        let mut app = app_with_fixture();
        app.showing_picker = true;
        handle_mouse_event(mouse(MouseEventKind::Moved, 5, 1), &mut app);
        assert_eq!(app.hovered_component, None);
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, key_modifiers::NONE)
    }

    // Task 3.1: help modal (`?`).
    //
    // `?` opens the help modal from any view; while open it eats all keys
    // except Esc/q (both close it and return false, so q does not quit), and a
    // left-button Down outside the published modal rect closes it over any
    // surface (design D1/D2/D4).

    #[test]
    fn question_mark_opens_help_from_panels() {
        let mut app = App::new();
        assert!(!app.showing_help);
        let quit = handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(!quit);
        assert!(app.showing_help);
    }

    #[test]
    fn question_mark_opens_help_from_every_view() {
        // Panels (default) is covered above; exercise the other surfaces.
        let mut app = App::new();
        app.showing_viewer = true;
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the viewer");

        let mut app = App::new();
        app.showing_graph = true;
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the graph");

        let mut app = App::new();
        app.showing_validation = true;
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the validation modal");

        let mut app = App::new();
        app.patch = Some(
            crate::patch::Patch::from_ini_str("[button]\n    button = B1.1\n", String::from("t"))
                .unwrap(),
        );
        assert!(app.open_optimizer());
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the optimizer");

        let mut app = App::new();
        app.showing_picker = true;
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the picker");
    }

    #[test]
    fn help_parity_keys_dispatch_through_handle_event() {
        // Task 2.2: every key synced into the help tables in 1.1-1.4 must
        // reach its handler branch without being swallowed by an earlier one,
        // and `?` must still open the modal over the surfaces those tables
        // document.

        // `?` opens the help modal over each surface whose table changed.
        for (label, app) in [
            ("panels", App::new()),
            ("picker", {
                let mut a = App::new();
                a.showing_picker = true;
                a
            }),
        ] {
            let mut app = app;
            handle_event(key(KeyCode::Char('?')), &mut app);
            assert!(app.showing_help, "? must open help over {label}");
        }
        let mut app = app_with_fixture();
        open_graph_slot(&mut app);
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "? must open help over the graph");
        let mut app = app_with_source_navigation();
        open_optimizer(&mut app);
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "? must open help over the optimizer");

        // Panels: `\` toggles the left-pane split, `g s` opens the select
        // menu, and Tab/Shift+Tab cycle pane focus while a slot is open.
        let mut app = App::new();
        handle_event(key(KeyCode::Char('\\')), &mut app);
        assert!(app.left_split_active, "\\ toggles the left split");
        assert_eq!(app.status_message, "Left split on");

        let mut app = app_with_select_patch();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert!(app.select_state.is_some(), "g s opens the select menu");

        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Slot(0));
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(
            app.tile_stack.focus,
            FocusSlot::Panels,
            "Tab cycles forward"
        );
        handle_event(shift_tab(), &mut app);
        assert_eq!(
            app.tile_stack.focus,
            FocusSlot::Slot(0),
            "Shift+Tab cycles backward"
        );

        // Graph: `h` toggles the arrangement, `f` the dependency filter, `i`
        // the influence filter, and `Alt+[`/`Alt+]` the cable tension.
        let mut app = app_with_fixture();
        open_graph_slot(&mut app);
        handle_event(key(KeyCode::Char('h')), &mut app);
        assert_eq!(app.layout_mode, crate::config::LayoutMode::Force);
        assert_eq!(app.status_message, "Layout: force");

        app.hovered_graph_node = Some(0);
        handle_event(key(KeyCode::Char('f')), &mut app);
        assert!(
            app.dependency_root.is_some(),
            "f engages the dependency filter"
        );

        app.select_component(String::from("B1.1"));
        assert!(
            app.influence.is_some(),
            "fixture seeds an influence subtree"
        );
        handle_event(key(KeyCode::Char('i')), &mut app);
        assert!(
            app.influence_filter_active,
            "i engages the influence filter"
        );

        // `g s` also opens the select menu from the graph surface.
        let mut app = app_with_select_patch();
        open_graph_slot(&mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert!(
            app.select_state.is_some(),
            "g s opens the select menu on the graph"
        );

        // Tension is a force-path control: run the force solver so `Alt+[`/
        // `Alt+]` re-solve and report.
        let mut app = app_with_fixture();
        app.layout_mode = crate::config::LayoutMode::Force;
        open_graph_slot(&mut app);
        let default = crate::layout::DEFAULT_TENSION;
        handle_event(alt_key(KeyCode::Char(']')), &mut app);
        assert_eq!(
            app.tension,
            default + crate::layout::TENSION_STEP,
            "Alt+] raises cable tension"
        );
        handle_event(alt_key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.tension, default, "Alt+[ lowers cable tension");

        // Physical: `+`/`-` cycle zoom presets, arrows pan an overflowing
        // rack, and j/k navigate without panning.
        let mut app = app_with_overflowing_rack();
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 1.5, "+ steps the zoom preset");
        handle_event(key(KeyCode::Char('-')), &mut app);
        assert_eq!(app.scale_factor, 1.0, "- steps the zoom preset back");
        // Focus the physical pane so arrows route to its pan.
        handle_event(key(KeyCode::Char('s')), &mut app);
        handle_event(key(KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (8.0, 0.0), "Right pans the rack");
        handle_event(key(KeyCode::Left), &mut app);
        assert_eq!(app.physical_offset, (0.0, 0.0), "Left pans back");
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.hovered_component, Some(1), "j navigates");
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.hovered_component, Some(0), "k navigates");

        // Picker: Ctrl+f latches the filter.
        let mut app = App::new();
        app.showing_picker = true;
        assert!(!handle_event(ctrl_f_key(), &mut app));
        assert!(app.picker_filter_active, "Ctrl+f latches the picker filter");

        // Optimizer: 0/1 snap the objective weight to its endpoints.
        let mut app = app_with_source_navigation();
        open_optimizer(&mut app);
        handle_event(key(KeyCode::Char('1')), &mut app);
        assert_eq!(
            app.optimizer.as_ref().unwrap().weight,
            1.0,
            "1 snaps the weight to 1.0"
        );
        handle_event(key(KeyCode::Char('0')), &mut app);
        assert_eq!(
            app.optimizer.as_ref().unwrap().weight,
            0.0,
            "0 snaps the weight to 0.0"
        );
    }

    #[test]
    fn esc_closes_help_and_returns_false() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        let quit = handle_event(key(KeyCode::Esc), &mut app);
        assert!(!quit);
        assert!(!app.showing_help);
    }

    #[test]
    fn q_closes_help_without_quitting() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        // q while help is open closes help and does NOT quit.
        let quit = handle_event(key(KeyCode::Char('q')), &mut app);
        assert!(!quit, "q must not quit while help is open");
        assert!(!app.showing_help);
    }

    #[test]
    fn help_eats_other_keys() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        // A random key while help is open is swallowed (returns false, no quit).
        let quit = handle_event(key(KeyCode::Char('l')), &mut app);
        assert!(!quit);
        assert!(app.showing_help, "help stays open on unrelated keys");
    }

    #[test]
    fn click_outside_closes_help() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        // Publish a modal rect, then click well outside it.
        app.help_modal_rect = Some(Rect::new(10, 10, 40, 20));
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, 2),
            &mut app,
        );
        assert!(!app.showing_help, "click outside the modal must close help");
    }

    #[test]
    fn click_inside_keeps_help_open() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        app.help_modal_rect = Some(Rect::new(10, 10, 40, 20));
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 20, 15),
            &mut app,
        );
        assert!(
            app.showing_help,
            "click inside the modal must keep help open"
        );
    }

    #[test]
    fn plus_and_minus_cycle_scale_presets_with_status() {
        let mut app = App::new();
        // From the 100% default, '-' steps down one preset to 75%.
        handle_event(key(KeyCode::Char('-')), &mut app);
        assert_eq!(app.scale_factor, 0.75);
        assert_eq!(app.status_message, "Scaling: 75%");

        // '+' climbs back through the presets.
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 1.0);
        assert_eq!(app.status_message, "Scaling: 100%");
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 1.5);
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 2.0);

        // At the top preset, '+' wraps around to the bottom.
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 0.75);
    }

    // Task 2.3: graph camera persistent state + wheel/arrow zoom-pan.
    //
    // A camera must be seeded (like the renderer does on the first kitty frame)
    // before zoom/pan apply; on the box-drawing path the camera is `None` so
    // both are no-ops and preserve the old navigation behavior.

    fn seed_graph_camera(app: &mut App) {
        use crate::graph_render::{GraphCamera, WorldBounds};
        let node_world = app.graph_fit_node_world();
        app.graph_camera = Some(GraphCamera::fit_to_world_with_nodes(
            WorldBounds::from_positions(&app.graph_positions),
            node_world,
            (960.0, 480.0),
            App::GRAPH_MIN_NODE_PX,
        ));
        app.graph_canvas_px = Some((960.0, 480.0));
    }

    fn alt_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, key_modifiers::ALT)
    }

    fn shift_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, key_modifiers::SHIFT)
    }

    // ---- Class-layout pane keys (change `pane-class-layout`, task 3.1) ----

    #[test]
    fn z_toggles_maximize_of_the_focused_pane() {
        use crate::panes::PaneId;
        let mut app = app_with_fixture();
        assert!(app.layout.maximized.is_none());
        // Default focus is the left big pane holding the module UI.
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert_eq!(app.layout.maximized, Some(PaneId::BigLeft));
        assert_eq!(app.status_message, "Maximized");
        // A second `z` restores the arrangement with the same views.
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert!(app.layout.maximized.is_none());
        assert_eq!(app.status_message, "Layout restored");
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
    }

    #[test]
    fn z_maximizes_the_focused_small_pane() {
        use crate::panes::PaneId;
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Tab), &mut app); // -> SmallTop (source viewer)
        assert_eq!(app.layout.focus, PaneId::SmallTop);
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert_eq!(app.layout.maximized, Some(PaneId::SmallTop));
    }

    #[test]
    fn focus_change_clears_the_maximize() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert!(app.layout.maximized.is_some());
        handle_event(key(KeyCode::Tab), &mut app);
        assert!(app.layout.maximized.is_none());
    }

    #[test]
    fn esc_clears_maximize_before_closing_the_view() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert!(app.layout.maximized.is_some());
        let view_before = app.layout.big_left.view;
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.layout.maximized.is_none());
        assert_eq!(app.layout.big_left.view, view_before, "view must stay open");
        assert_eq!(app.status_message, "Layout restored");
    }

    #[test]
    fn esc_clears_maximize_even_with_an_empty_mirror_slot_set() {
        use crate::panes::PaneId;
        let mut app = app_with_fixture();
        // Close the source viewer so the mirror has no slots; the tiled Esc
        // dispatch below would not run in that state.
        handle_event(key(KeyCode::Tab), &mut app); // focus SmallTop
        handle_event(key(KeyCode::Esc), &mut app); // close source viewer
        assert!(app.tile_stack.slots.is_empty());
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert_eq!(app.layout.maximized, Some(PaneId::SmallTop));
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.layout.maximized.is_none());
        assert_eq!(app.status_message, "Layout restored");
    }

    #[test]
    fn alt_b_promotes_the_focused_small_view_into_the_big_pane() {
        use crate::panes::PaneId;
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Tab), &mut app); // focus SmallTop (source viewer)
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
        handle_event(alt_key(KeyCode::Char('b')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::SourceViewer));
        assert_eq!(app.layout.small_top.view, Some(ViewType::Physical));
        assert_eq!(app.layout.focus, PaneId::SmallTop, "focus is preserved");
    }

    #[test]
    fn alt_b_reports_noop_when_the_only_big_pane_is_focused() {
        let mut app = app_with_fixture();
        // Startup small arrangement: BigLeft is the only big pane and holds focus.
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        handle_event(alt_key(KeyCode::Char('b')), &mut app);
        assert_eq!(app.status_message, "No swap applies");
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
    }

    #[test]
    fn alt_s_exchanges_the_two_small_panes() {
        let mut app = app_with_fixture();
        // Default: SmallTop holds the source viewer, SmallBottom is empty.
        handle_event(alt_key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.layout.small_top.view, None);
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::SourceViewer));
    }

    #[test]
    fn alt_s_exchanges_a_promoted_view_pair() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('o')), &mut app); // optimizer -> SmallBottom
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::Optimizer));
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
        handle_event(alt_key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.layout.small_top.view, Some(ViewType::Optimizer));
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::SourceViewer));
    }

    #[test]
    fn alt_s_reports_noop_without_small_panes() {
        let mut app = app_with_fixture();
        // Close the only small-class view: the right half becomes one big pane.
        handle_event(key(KeyCode::Tab), &mut app); // focus SmallTop
        handle_event(key(KeyCode::Esc), &mut app); // close source viewer
        assert!(!app.layout.has_small_view());
        handle_event(alt_key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.status_message, "No swap applies");
    }

    #[test]
    fn help_modal_consumes_the_pane_layout_keys() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert!(app.layout.maximized.is_none(), "help eats z");
        handle_event(alt_key(KeyCode::Char('b')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        handle_event(alt_key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
    }

    #[test]
    fn picker_consumes_the_pane_layout_keys() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('l')), &mut app);
        assert!(app.showing_picker);
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert!(app.layout.maximized.is_none());
        handle_event(alt_key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
    }

    // Task 4.3 handler parity: the centered overlays (validation modal, label
    // editor) consume the pane-layout keys exactly like the help modal and the
    // picker (keybinding spec "SHALL NOT fire while ... the validation modal,
    // the label overlay ... has focus").
    #[test]
    fn validation_modal_consumes_the_pane_layout_keys() {
        let mut app = app_with_fixture();
        app.showing_validation = true;
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert!(app.layout.maximized.is_none(), "validation eats z");
        handle_event(alt_key(KeyCode::Char('b')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        handle_event(alt_key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
        assert!(app.showing_validation, "the modal stays open");
    }

    #[test]
    fn label_overlay_consumes_the_pane_layout_keys() {
        let mut app = app_with_fixture();
        app.editing = Some(crate::app::EditState::new_hw(
            String::from("B1.1"),
            1,
            String::new(),
        ));
        handle_event(key(KeyCode::Char('z')), &mut app);
        assert!(app.layout.maximized.is_none(), "label overlay eats z");
        handle_event(alt_key(KeyCode::Char('b')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        handle_event(alt_key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
        assert!(app.editing.is_some(), "the overlay stays open");
    }

    // Spec "Swap the two big panes": with no small view open both halves are
    // big panes, and `Alt+b` exchanges their views while focus stays put.
    #[test]
    fn alt_b_exchanges_the_two_big_panes_through_dispatch() {
        use crate::panes::PaneId;
        let mut app = app_with_fixture();
        // Two big panes (no small view): module UI left, graph right. Set the
        // arrangement and its mirror together so `reconcile_from_mirror` keeps
        // it, then exercise the real Alt+b dispatch.
        app.layout.big_right.view = Some(ViewType::Graph);
        app.layout.small_top.view = None;
        app.tile_stack.slots = vec![ViewType::Graph];
        app.tile_stack.focus = FocusSlot::Panels;
        assert_eq!(app.layout.focus, PaneId::BigLeft);

        handle_event(alt_key(KeyCode::Char('b')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Graph));
        assert_eq!(app.layout.big_right.view, Some(ViewType::Physical));
        assert_eq!(app.layout.focus, PaneId::BigLeft, "focus preserved");
    }

    #[test]
    fn graph_plus_cycles_zoom_presets_and_wraps() {
        let mut app = app_with_fixture();
        app.open_graph();
        // Direct `open_graph` leaves focus on panels; `g g` focuses the
        // graph slot, which the zoom arm requires.
        app.tile_stack.focus = FocusSlot::Slot(0);
        seed_graph_camera(&mut app);

        let z0 = app.graph_camera.unwrap().zoom;
        handle_event(key(KeyCode::Char('+')), &mut app);
        let z1 = app.graph_camera.unwrap().zoom;
        assert!(
            (z1 - z0 * 1.5).abs() < 1e-2,
            "'+' zooms in one preset: {z0} -> {z1}"
        );
        // Presets are multipliers of the fitted zoom: 1.0 is
        // GRAPH_ZOOM_FIT_INDEX (8), one '+' lands on 1.5 (index 9).
        assert_eq!(app.graph_zoom_preset, App::GRAPH_ZOOM_FIT_INDEX as u8 + 1);
        assert!(app.status_message.contains("Graph zoom"));

        // Wrap at the top preset (800%) back to the bottom (0.78125%):
        // the deep zoom-out steps exist so a fitted camera can reach the
        // true fit of a large patch.
        // Step through: 1.5 -> 2.0 -> 4.0 -> 8.0 -> wrap to 0.0078125
        for _ in 0..4 {
            handle_event(key(KeyCode::Char('+')), &mut app);
        }
        let z_wrapped = app.graph_camera.unwrap().zoom;
        assert!(
            (z_wrapped - 0.0078125).abs() < 1e-2,
            "wrap from 800% to 0.78125%: {z0} -> {z_wrapped}"
        );
        assert_eq!(app.graph_zoom_preset, 0);
    }

    #[test]
    fn graph_arrows_pan_and_zoom_reemits_next_frame() {
        let mut app = app_with_fixture();
        app.open_graph();
        seed_graph_camera(&mut app);
        // Camera fit centers the content; force an overflow so pan is allowed.
        // The canvas must be small enough to overflow even for the compact
        // layered seed (a tall fan-out graph fits height-first, so its world
        // width at the fitted zoom is small).
        let before = app.graph_camera.unwrap().pan;
        app.graph_canvas_px = Some((20.0, 20.0));
        handle_event(key(KeyCode::Right), &mut app);
        let after = app.graph_camera.unwrap().pan;
        assert!(
            (after.0 - before.0).abs() > 1.0,
            "arrow pans the camera when it overflows: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn graph_zoom_pan_are_noop_until_camera_seeded() {
        // Without a seeded camera (box-drawing path), '+' and arrows must not
        // panic and must leave the camera None (old navigation preserved).
        let mut app = app_with_fixture();
        app.open_graph();
        app.tile_stack.focus = FocusSlot::Slot(0);
        assert!(app.graph_camera.is_none());
        handle_event(key(KeyCode::Char('+')), &mut app);
        handle_event(key(KeyCode::Up), &mut app);
        assert!(app.graph_camera.is_none());
        assert!(app.showing_graph);
    }

    #[test]
    fn open_graph_resets_the_camera_to_unfitted() {
        let mut app = app_with_fixture();
        // A stale camera from a prior graph must not survive a re-open.
        app.graph_camera = Some(crate::graph_render::GraphCamera::default());
        app.graph_zoom_preset = 3;
        app.graph_canvas_px = Some((1280.0, 720.0));
        app.open_graph();
        assert!(app.graph_camera.is_none());
        // Default preset is the fitted zoom (1.0, GRAPH_ZOOM_FIT_INDEX), not
        // a stale one.
        assert_eq!(app.graph_zoom_preset, App::GRAPH_ZOOM_FIT_INDEX as u8);
        assert!(app.graph_canvas_px.is_none());
    }

    #[test]
    fn c_centers_graph_camera_on_graph_surface() {
        // Design D3: bare `c` pans so the drawn bounds' center maps to the
        // visible pane's center. Zoom stays untouched, and latency coloring
        // is no longer flipped by the bare key (it moved to the `g c` chord).
        let mut app = app_with_fixture();
        app.open_graph();
        // Seed an off-center camera + canvas, as the renderer publishes them.
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 2.0,
            pan: (10.0, 20.0),
        });
        app.graph_canvas_px = Some((640.0, 300.0));
        let latency_before = app.latency_coloring;

        handle_event(key(KeyCode::Char('c')), &mut app);

        let cam = app.graph_camera.unwrap();
        assert_eq!(cam.zoom, 2.0, "centering must not change zoom");
        assert_ne!(cam.pan, (10.0, 20.0), "centering must pan");
        assert_eq!(app.status_message, "Graph centered");
        assert_eq!(
            app.latency_coloring, latency_before,
            "bare c must not flip latency coloring"
        );
    }

    #[test]
    fn c_centering_is_noop_until_camera_and_canvas_are_published() {
        // Mirror of the app.rs contract: without a published camera or canvas
        // size the center is a silent no-op (box-drawing path) — no pan, no
        // status change.
        let mut app = app_with_fixture();
        app.open_graph();
        assert!(app.graph_camera.is_none());

        handle_event(key(KeyCode::Char('c')), &mut app);

        assert!(app.graph_camera.is_none());
        assert_ne!(app.status_message, "Graph centered");
    }

    #[test]
    fn shift_c_refits_graph_camera_against_published_canvas() {
        // Design D3/D4: Shift+c is a full refit against the published pane
        // size, and the zoom preset resets to the fitted-zoom entry so the
        // `+`/`-` cycle and the first-frame fit agree.
        let mut app = app_with_fixture();
        app.open_graph();
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 9.0,
            pan: (123.0, 456.0),
        });
        app.graph_zoom_preset = 0;
        app.graph_canvas_px = Some((640.0, 300.0));

        handle_event(shift_key(KeyCode::Char('C')), &mut app);

        assert_eq!(app.graph_zoom_preset, App::GRAPH_ZOOM_FIT_INDEX as u8);
        assert_eq!(app.graph_canvas_px, Some((640.0, 300.0)));
        assert!(
            app.status_message.contains("Graph zoom"),
            "unexpected status: {:?}",
            app.status_message
        );
        let cam = app.graph_camera.unwrap();
        assert!(cam.zoom > 0.0 && cam.zoom.is_finite(), "refit zooms");
    }

    #[test]
    fn shift_c_falls_back_to_default_viewport_before_first_publish() {
        // Before the renderer publishes a canvas size the refit uses the
        // default viewport and publishes it, so later keys anchor on it.
        let mut app = app_with_fixture();
        app.open_graph();
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 9.0,
            pan: (123.0, 456.0),
        });
        assert!(app.graph_canvas_px.is_none());

        handle_event(shift_key(KeyCode::Char('C')), &mut app);

        assert_eq!(app.graph_canvas_px, Some((1280.0, 800.0)));
        assert_eq!(app.graph_zoom_preset, App::GRAPH_ZOOM_FIT_INDEX as u8);
    }

    #[test]
    fn g_c_toggles_latency_coloring_with_existing_status_text() {
        // Design D3: latency coloring moved to the `g c` chord; the status
        // text keeps its existing shape.
        let mut app = app_with_fixture();
        open_graph_slot(&mut app);
        assert!(app.latency_coloring, "latency coloring on by default");

        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('c')), &mut app);

        assert!(!app.latency_coloring);
        assert_eq!(app.status_message, "Latency coloring off (g c to toggle)");

        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('c')), &mut app);

        assert!(app.latency_coloring);
        assert_eq!(app.status_message, "Latency coloring on (g c to toggle)");
    }

    /// Open the graph slot via `g` then `g`.
    fn open_graph_slot(app: &mut App) {
        handle_event(key(KeyCode::Char('g')), app);
        handle_event(key(KeyCode::Char('g')), app);
        assert!(app.showing_graph);
    }

    #[test]
    fn f_engages_dependency_filter_on_hovered_node() {
        let mut app = app_with_fixture();
        open_graph_slot(&mut app);
        let root = app.graph.as_ref().unwrap().nodes[0].id.clone();
        app.hovered_graph_node = Some(0);
        handle_event(key(KeyCode::Char('f')), &mut app);
        assert_eq!(app.dependency_root, Some(root));
        assert!(!app.dependency_nodes.is_empty());
        assert!(
            app.status_message.starts_with("Dependencies of "),
            "unexpected status: {:?}",
            app.status_message
        );
        // The subset solves deterministically: every subset node has a finite
        // position in the full-length array.
        for &i in &app.dependency_nodes {
            let (x, y) = app.graph_positions[i];
            assert!(x.is_finite() && y.is_finite(), "node {i} unsolved");
        }
    }

    #[test]
    fn f_falls_back_to_circuit_selection_when_nothing_hovered() {
        let mut app = app_with_fixture();
        open_graph_slot(&mut app);
        let root = app.graph.as_ref().unwrap().nodes[0].id.clone();
        app.selected_circuit = Some(root.clone());
        handle_event(key(KeyCode::Char('f')), &mut app);
        assert_eq!(app.dependency_root, Some(root));
    }

    #[test]
    fn f_without_hover_or_selection_hints_and_keeps_full_graph() {
        let mut app = app_with_fixture();
        open_graph_slot(&mut app);
        handle_event(key(KeyCode::Char('f')), &mut app);
        assert!(app.dependency_root.is_none());
        assert_eq!(app.status_message, "No graph node selected");
    }

    #[test]
    fn f_toggles_off_and_esc_clears_the_filter() {
        let mut app = app_with_fixture();
        open_graph_slot(&mut app);
        app.hovered_graph_node = Some(0);
        handle_event(key(KeyCode::Char('f')), &mut app);
        assert!(app.dependency_root.is_some());
        // A second `f` restores the full graph.
        handle_event(key(KeyCode::Char('f')), &mut app);
        assert!(app.dependency_root.is_none());
        assert!(app.dependency_nodes.is_empty());
        // Re-engage, then Esc clears without closing the graph slot.
        app.hovered_graph_node = Some(0);
        handle_event(key(KeyCode::Char('f')), &mut app);
        assert!(app.dependency_root.is_some());
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.dependency_root.is_none());
        assert!(app.showing_graph, "Esc clears the filter, not the graph");
    }

    #[test]
    fn picker_enter_on_parent_entry_navigates_up_without_closing() {
        let mut app = picker_app_at("fixtures/picker_test");
        assert!(
            is_picker_parent_entry(&app.picker_entries[0]),
            "parent entry is the '..' sentinel"
        );
        app.picker_index = 0;
        handle_picker_event(key(KeyCode::Enter), &mut app);
        assert!(app.showing_picker, "picker stays open when navigating up");
        assert_eq!(app.picker_dir, std::path::PathBuf::from("fixtures"));
        assert!(app.patch.is_none());
    }

    /// Open the embedded source viewer via `g` then `v`.
    fn open_viewer(app: &mut App) {
        handle_event(key(KeyCode::Char('g')), app);
        handle_event(key(KeyCode::Char('v')), app);
        assert!(app.showing_viewer);
    }

    #[test]
    fn bracket_split_keys_adjust_the_big_boundary_with_no_viewer() {
        // Class layout: `[`/`]` move the left/right big-pane boundary from the
        // even 0.5 default, independent of any open view.
        let mut app = App::new();
        assert_eq!(app.layout.main_split_ratio, 0.5);
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.4);
        assert_eq!(app.status_message, "Split: 40%/60%");
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.5);
    }

    #[test]
    fn close_bracket_increases_big_boundary_and_clamps_at_0_7() {
        let mut app = App::new();
        // 0.5 -> 0.6 -> 0.7 (clamp).
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.6);
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.7);
        assert_eq!(app.main_split_ratio, 0.7, "mirror stays synced");
        assert_eq!(app.status_message, "Split: 70%/30%");
        // Further presses clamp at the upper bound.
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.7);
    }

    #[test]
    fn open_bracket_decreases_big_boundary_and_clamps_at_0_3() {
        let mut app = App::new();
        // 0.5 -> 0.4 -> 0.3 (clamp).
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.4);
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.3);
        assert_eq!(app.status_message, "Split: 30%/70%");
        // Further presses clamp at the lower bound.
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.3);
    }

    /// Open the optimizer menu via `g` then `o`.
    fn open_optimizer(app: &mut App) {
        handle_event(key(KeyCode::Char('g')), app);
        handle_event(key(KeyCode::Char('o')), app);
        assert!(app.optimizer.is_some());
    }

    #[test]
    fn g_o_opens_optimizer_menu_with_candidates() {
        let mut app = app_with_fixture();
        open_optimizer(&mut app);
        let state = app.optimizer.as_ref().unwrap();
        assert!(!state.candidates.is_empty());
        assert!(state.candidates.len() <= 3);
        assert_eq!(state.cursor, 0);
        assert_eq!(
            state.original_order.len(),
            app.patch.as_ref().unwrap().sections.len()
        );
    }

    #[test]
    fn g_o_without_patch_shows_hint_and_keeps_menu_closed() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('o')), &mut app);
        assert!(app.optimizer.is_none());
        assert!(
            app.status_message.contains("No patch loaded"),
            "unexpected status: {:?}",
            app.status_message
        );
    }

    #[test]
    fn optimizer_jk_navigate_and_enter_previews_and_restores() {
        let mut app = app_with_fixture();
        open_optimizer(&mut app);
        let n = app.optimizer.as_ref().unwrap().candidates.len();
        if n < 2 {
            return;
        }
        // j moves down (wraps at the end), k moves up.
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().cursor, 1);
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().cursor, 0);

        // Enter previews candidate 0: sections reordered, graph rebuilt.
        let original: Vec<String> = app
            .patch
            .as_ref()
            .unwrap()
            .sections
            .iter()
            .map(|s| s.name.clone())
            .collect();
        handle_event(key(KeyCode::Enter), &mut app);
        let previewed = app.optimizer.as_ref().unwrap().previewing;
        assert_eq!(previewed, Some(0));
        assert!(app.graph.is_some());
        assert!(
            app.status_message.contains("Preview:"),
            "unexpected status: {:?}",
            app.status_message
        );

        // r restores the original order.
        handle_event(key(KeyCode::Char('r')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().previewing, None);
        let restored: Vec<String> = app
            .patch
            .as_ref()
            .unwrap()
            .sections
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(restored, original);
    }

    #[test]
    fn optimizer_esc_closes_and_restores_preview() {
        let mut app = app_with_fixture();
        open_optimizer(&mut app);
        let original: Vec<String> = app
            .patch
            .as_ref()
            .unwrap()
            .sections
            .iter()
            .map(|s| s.name.clone())
            .collect();
        handle_event(key(KeyCode::Enter), &mut app);
        assert!(app.optimizer.as_ref().unwrap().previewing.is_some());
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.optimizer.is_none());
        let after: Vec<String> = app
            .patch
            .as_ref()
            .unwrap()
            .sections
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(after, original, "Esc must restore the file order");
    }

    #[test]
    fn optimizer_weight_keys_step_snap_and_clamp() {
        // source_navigation.ini is weight-sensitive: its best ordering under
        // the weighted objective differs from the pure MinSum one, so stepping
        // w must change the candidate summaries (arpeggio1 ties everywhere).
        let mut app = app_with_source_navigation();
        open_optimizer(&mut app);
        // Starts at the MinSum endpoint.
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.0);
        let candidates_at_0 = app.optimizer.as_ref().unwrap().candidates.clone();

        // `]` steps +0.1; the status line reports the new weight.
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.1);
        assert!(
            app.status_message.contains("w = 0.1"),
            "status reports w: {:?}",
            app.status_message
        );
        // Candidates are regenerated under the weighted objective: the best
        // ordering can differ from the MinSum one on this patch.
        let candidates_at_01 = app.optimizer.as_ref().unwrap().candidates.clone();
        assert_ne!(
            candidates_at_01[0].order, candidates_at_0[0].order,
            "weighted objective must change the best candidate on this fixture"
        );

        // `[` steps back down to the MinSum endpoint.
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.0);

        // `0`/`1` snap straight to the endpoints.
        handle_event(key(KeyCode::Char('1')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 1.0);
        handle_event(key(KeyCode::Char('0')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.0);

        // `]` past the top clamps at 1.0; `[` past the bottom clamps at 0.0.
        handle_event(key(KeyCode::Char('1')), &mut app);
        for _ in 0..5 {
            handle_event(key(KeyCode::Char(']')), &mut app);
        }
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 1.0);
        // Ten steps down from 1.0 reach 0.0; an eleventh `[` must stay at the floor.
        for _ in 0..11 {
            handle_event(key(KeyCode::Char('[')), &mut app);
        }
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.0);

        // Esc still closes the menu; `g o` still reopens it.
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.optimizer.is_none());
        open_optimizer(&mut app);
        assert!(app.optimizer.is_some());
    }

    #[test]
    fn optimizer_weight_keys_do_not_shift_viewer_split() {
        // The optimizer pane owns `[`/`]` while focused (spec "Weight key
        // stays view-local"); closing it hands the boundary keys back.
        let mut app = app_with_fixture();
        open_optimizer(&mut app);
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(
            app.layout.main_split_ratio, 0.5,
            "optimizer `]` must not move the big boundary"
        );
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.1);
        handle_event(key(KeyCode::Esc), &mut app);
        // With the optimizer closed `]` moves the big boundary again.
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.6);
    }

    #[test]
    fn optimizer_export_writes_latopt_file_next_to_source() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let content = std::fs::read_to_string("fixtures/cable_banner_combos.ini").unwrap();
        let src = dir.path().join("patch.ini");
        std::fs::write(&src, &content).unwrap();
        let patch = Patch::from_ini_file(&src).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.current_patch_path = Some(src.clone());
        open_optimizer(&mut app);
        let n = app.optimizer.as_ref().unwrap().candidates.len();
        if n == 0 {
            return;
        }
        handle_event(key(KeyCode::Char('s')), &mut app);
        let dest = dir.path().join("patch-latopt.ini");
        assert!(
            dest.exists(),
            "expected exported file at {:?}",
            dest.display()
        );
        // The export re-parses and keeps the same section set.
        let reparsed = Patch::from_ini_file(&dest).unwrap();
        assert_eq!(
            reparsed.sections.len(),
            app.patch.as_ref().unwrap().sections.len()
        );
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        let quit = handle_event(
            KeyEvent::new(KeyCode::Char('c'), key_modifiers::CONTROL),
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

        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.hovered_component, Some(2));
    }

    #[test]
    fn keyboard_toggle_and_mouse_click_agree_on_target() {
        let mut app = app_with_fixture();
        handle_mouse_event(mouse(MouseEventKind::Moved, 5, 1), &mut app);
        assert_eq!(app.hovered_component, Some(0));

        // Enter (keyboard) toggles whatever is currently hovered, same as a click would.
        handle_event(key(KeyCode::Enter), &mut app);
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
            handle_event(key(KeyCode::Char(ch)), &mut app);
            assert_eq!(app.active_shift, Some(expected));
        }
        handle_event(key(KeyCode::Esc), &mut app);
        assert_eq!(app.active_shift, None);
    }

    fn picker_app_at(dir: &str) -> App {
        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
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

    fn ctrl_f_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('f'), key_modifiers::CONTROL)
    }

    fn type_picker_chars(app: &mut App, chars: &str) {
        for c in chars.chars() {
            handle_picker_event(KeyEvent::new(KeyCode::Char(c), key_modifiers::NONE), app);
        }
    }

    #[test]
    fn picker_ctrl_f_latches_filter_with_empty_string() {
        let mut app = picker_app_at("fixtures/picker_test");
        assert!(!app.picker_filter_active);
        let quit = handle_picker_event(ctrl_f_key(), &mut app);
        assert!(!quit);
        assert!(app.picker_filter_active);
        assert!(app.picker_filter.is_empty());
        assert_eq!(app.status_message, "Filter: on (type to filter)");
        // Latching: the picker stays open with the full listing unchanged.
        assert!(app.showing_picker);
    }

    #[test]
    fn picker_filter_chars_append_and_narrow_listing() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "patch_a");
        assert_eq!(app.picker_filter, "patch_a");
        let names: Vec<String> = app
            .picker_entries
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("patch_a.ini")));
        assert!(!names.iter().any(|n| n.ends_with("patch_b.ini")));
        assert!(!names.iter().any(|n| n.ends_with("readme.txt")));
        assert!(!names.iter().any(|n| n.ends_with("subdir")));
        // The `..` sentinel stays visible for up-navigation.
        assert!(app.picker_entries.iter().any(|p| is_picker_parent_entry(p)));
    }

    #[test]
    fn picker_backspace_pops_last_filter_char() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "abba");
        assert_eq!(app.picker_filter, "abba");
        handle_picker_event(key(KeyCode::Backspace), &mut app);
        assert_eq!(app.picker_filter, "abb");
        // Backspacing down to the empty string restores the full listing.
        for _ in 0..3 {
            handle_picker_event(key(KeyCode::Backspace), &mut app);
        }
        assert!(app.picker_filter.is_empty());
        assert!(app.picker_filter_active, "backspace keeps the latch on");
        let names: Vec<String> = app
            .picker_entries
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("readme.txt")));
    }

    #[test]
    fn picker_digits_still_fast_select_favourites_while_filter_active() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.favorites = crate::favorites::FavoritesStore {
            favourites: vec![
                String::from("fixtures/picker_test/patch_a.ini"),
                String::from("fixtures/picker_test/patch_b.ini"),
            ],
        };
        app.refresh_picker_entries();
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "p");
        // Out-of-range digit: silent, and NOT appended to the filter string.
        handle_picker_event(key(KeyCode::Char('9')), &mut app);
        assert_eq!(app.picker_filter, "p");
        assert!(app.showing_picker);
        // Slot 0 fast-selects the first sorted favourite (patch_a.ini).
        handle_picker_event(key(KeyCode::Char('0')), &mut app);
        assert!(!app.showing_picker);
        assert_eq!(app.patch.as_ref().unwrap().name, "patch_a");
        assert!(!app.picker_filter_active);
    }

    #[test]
    fn picker_q_resets_filter_without_quitting() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "patch_a");
        assert!(app.picker_filter_active);
        let quit = handle_picker_event(key(KeyCode::Char('q')), &mut app);
        assert!(!quit, "q must not quit while the picker is open");
        assert!(app.showing_picker);
        assert!(!app.picker_filter_active);
        assert!(app.picker_filter.is_empty());
        let names: Vec<String> = app
            .picker_entries
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        // After q resets the filter the full root listing is restored; the
        // fixture root contains patch_a.ini and subdir/patch_b is not in the
        // root listing, so check for a root entry that was filtered away.
        assert!(names.iter().any(|n| n.ends_with("patch_a.ini")));
        assert!(names.iter().any(|n| n.ends_with("subdir")));
    }

    #[test]
    fn picker_esc_resets_filter_and_closes() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "abba");
        assert!(app.picker_filter_active);
        let quit = handle_picker_event(key(KeyCode::Esc), &mut app);
        assert!(!quit);
        assert!(!app.showing_picker);
        assert!(!app.picker_filter_active);
        assert!(app.picker_filter.is_empty());
    }

    #[test]
    fn picker_enter_resets_filter_on_patch_load() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "patch_a");
        app.picker_index = picker_index_of(&app, "patch_a.ini");
        handle_picker_event(key(KeyCode::Enter), &mut app);
        assert!(!app.showing_picker);
        assert_eq!(app.patch.as_ref().unwrap().name, "patch_a");
        assert!(!app.picker_filter_active);
        assert!(app.picker_filter.is_empty());
    }

    #[test]
    fn picker_jk_bounds_hold_on_filtered_list() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "subdir");
        let len = app.picker_entries.len();
        assert_eq!(
            len, 2,
            "sentinel + matching dir only: {:?}",
            app.picker_entries
        );
        // j stops at the bottom bound; k stops at the top bound.
        handle_picker_event(key(KeyCode::Char('j')), &mut app);
        handle_picker_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.picker_index, len - 1);
        handle_picker_event(key(KeyCode::Char('k')), &mut app);
        handle_picker_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.picker_index, 0);
    }

    #[test]
    fn picker_ctrl_f_re_latch_clears_the_string() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_event(ctrl_f_key(), &mut app);
        type_picker_chars(&mut app, "abba");
        // Second Ctrl+f toggles off per spec (filter latching toggles on/off).
        handle_picker_event(ctrl_f_key(), &mut app);
        assert!(!app.picker_filter_active);
        assert!(app.picker_filter.is_empty());
        assert_eq!(app.status_message, "Filter: off");
    }

    #[test]
    fn picker_esc_cancels() {
        let mut app = picker_app_at("fixtures/picker_test");
        let quit = handle_picker_event(key(KeyCode::Esc), &mut app);
        assert!(!quit);
        assert!(!app.showing_picker);
    }

    #[test]
    fn picker_enter_on_ini_loads_and_closes() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.picker_index = picker_index_of(&app, "patch_a.ini");
        handle_picker_event(key(KeyCode::Enter), &mut app);
        assert!(!app.showing_picker);
        assert_eq!(app.patch.as_ref().unwrap().name, "patch_a");
    }

    #[test]
    fn picker_enter_keys_label_store_path() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.picker_index = picker_index_of(&app, "patch_a.ini");
        handle_picker_event(key(KeyCode::Enter), &mut app);
        assert!(!app.showing_picker);
        assert!(app.patch.is_some());
        let path = app
            .current_patch_path
            .as_ref()
            .expect("current_patch_path set by picker load");
        assert!(path.ends_with("patch_a.ini"));
        // Validation ran on the picker load; the fixture is clean.
        assert!(app.validation_issues.is_empty());
    }

    #[test]
    fn picker_enter_gates_on_error_when_patch_loaded() {
        let mut app = app_with_fixture();
        app.picker_dir = std::path::PathBuf::from("fixtures/validation");
        app.showing_picker = true;
        app.refresh_picker_entries();
        app.picker_index = picker_index_of(&app, "ram_overflow.ini");
        handle_picker_event(key(KeyCode::Enter), &mut app);
        // The picker closes on a gated load so the validation modal is reachable.
        assert!(!app.showing_picker);
        // Gate keeps the previously loaded patch.
        assert_eq!(app.patch.as_ref().unwrap().name, "arpeggio1");
        assert!(app.showing_validation);
        assert!(app.validation_issues.iter().any(|i| {
            i.severity == crate::validation::Severity::Error && i.code != "unknown_param"
        }));
    }

    #[test]
    fn picker_enter_on_directory_navigates_in_without_closing() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.picker_index = picker_index_of(&app, "subdir");
        handle_picker_event(key(KeyCode::Enter), &mut app);
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
        handle_picker_event(key(KeyCode::Enter), &mut app);
        assert!(app.showing_picker);
        assert!(app.patch.is_none());
    }

    #[test]
    fn picker_j_k_navigation_stays_in_bounds() {
        let mut app = picker_app_at("fixtures/picker_test");
        let len = app.picker_entries.len();
        app.picker_index = 0;
        handle_picker_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.picker_index, 0); // clamped, doesn't go negative

        for _ in 0..len + 2 {
            handle_picker_event(key(KeyCode::Char('j')), &mut app);
        }
        assert_eq!(app.picker_index, len - 1); // clamped at the end
    }

    #[test]
    fn g_enters_prefix_mode() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
    }

    #[test]
    fn g_then_v_opens_viewer_and_clears_prefix() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert!(app.prefix.is_none());
    }
    #[test]
    fn g_then_v_initial_position_bof_when_no_selection() {
        let mut app = app_with_source_navigation();
        // No selection -> BOF
        app.source_scroll = 99;
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, 0);
        assert_eq!(app.occurrence_cursor, 0);
        assert!(app.selected_component.is_none());
    }

    #[test]
    fn g_then_v_jumps_to_first_occurrence_when_selected() {
        let mut app = app_with_source_navigation();
        let first = app.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
        app.select_component(String::from("B1.1"));
        // Move scroll away to prove jump
        app.source_scroll = 999;
        app.showing_viewer = false;
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, first);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.selected_component, Some(String::from("B1.1")));
    }

    #[test]
    fn g_then_other_key_cancels_prefix_and_processes_key_normally() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert!(app.prefix.is_none());
        assert_eq!(app.hovered_component, Some(1));
    }

    #[test]
    fn g_then_esc_cancels_prefix_without_other_action() {
        let mut app = App::new();
        app.active_shift = Some(ShiftGroup::Group1);
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.prefix.is_none());
        // Esc while a prefix is armed must not also clear the shift group.
        assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
    }

    // `g s` select-state menu (change C 4.1): opens the centered signal list,
    // j/k navigate the cursor, [/] cycle the focused signal's candidate with a
    // live graph rebuild, Esc clears the assumed state and restores the
    // unassumed graph.

    fn app_with_select_patch() -> App {
        let content = "\
[p2b8]\n\
[button]\n    select = S1.1\n    selectat = 0\n    button = B1.1\n\
[button]\n    select = _CABLE\n    button = B1.2\n\
[button]\n    button = B1.3\n";
        let patch = Patch::from_ini_str(content, String::from("select_fixture")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app
    }

    #[test]
    fn g_then_s_opens_select_menu_and_clears_prefix() {
        let mut app = app_with_select_patch();
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert!(app.prefix.is_none());
        assert!(app.select_state.is_some(), "g s opens the select menu");
    }

    #[test]
    fn select_menu_jk_navigate_cursor() {
        let mut app = app_with_select_patch();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.select_state.as_ref().unwrap().cursor, 0);
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.select_state.as_ref().unwrap().cursor, 1);
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(
            app.select_state.as_ref().unwrap().cursor,
            1,
            "cursor clamps at last"
        );
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.select_state.as_ref().unwrap().cursor, 0);
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(
            app.select_state.as_ref().unwrap().cursor,
            0,
            "cursor clamps at first"
        );
    }

    #[test]
    fn select_menu_brackets_cycle_focused_candidate() {
        let mut app = app_with_select_patch();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('s')), &mut app);
        // Focused signal is the first (register S1.1); default first candidate 0.
        assert_eq!(
            app.select_state.as_ref().unwrap().state.get("S1.1"),
            Some(&0.0)
        );
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(
            app.select_state.as_ref().unwrap().state.get("S1.1"),
            Some(&1.0),
            "] cycles to the next candidate"
        );
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert_eq!(
            app.select_state.as_ref().unwrap().state.get("S1.1"),
            Some(&0.0),
            "[ cycles back"
        );
    }

    #[test]
    fn select_menu_esc_clears_state_and_restores_default_graph() {
        let mut app = app_with_select_patch();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert!(app.select_state.is_some());
        // A mismatch (S1.1=1 vs selectat 0) drops bar's controller edge.
        handle_event(key(KeyCode::Char(']')), &mut app);
        let gated = app.graph.as_ref().unwrap();
        assert!(
            gated.not_selected.contains(&1),
            "bar section index 1 is NotSelected under S1.1=1"
        );
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.select_state.is_none());
        let restored = app.graph.as_ref().unwrap();
        assert!(
            restored.not_selected.is_empty(),
            "default build gates nothing"
        );
    }

    #[test]
    fn g_g_opens_graph_tile() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('g')), &mut app);
        let quit = handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(!quit);
        assert!(app.showing_graph, "`g g` opens the graph tile");
    }

    #[test]
    fn g_prefix_times_out_and_next_key_processed_normally() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('g')), &mut app);
        // Simulate an expired timeout window, then a key that should run
        // normally (navigation) instead of acting as a prefix follow-up.
        app.prefix = Some(PrefixState {
            started: Instant::now() - Duration::from_secs(2),
        });
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert!(app.prefix.is_none());
        assert_eq!(app.hovered_component, Some(1));
    }

    #[test]
    fn g_after_timeout_rearms_prefix() {
        // Non-interference (task 4.3): a `g` whose prefix already timed out
        // clears the stale prefix and re-arms a fresh one rather than acting
        // as a follow-up key.
        let mut app = App::new();
        handle_event(key(KeyCode::Char('g')), &mut app);
        app.prefix = Some(PrefixState {
            started: Instant::now() - Duration::from_secs(2),
        });
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
        assert!(app.prefix.as_ref().unwrap().started.elapsed() < Duration::from_secs(1));
        assert!(
            !app.showing_graph,
            "timed-out prefix must not open the graph"
        );
    }

    #[test]
    fn g_then_g_opens_graph_for_loaded_patch() {
        // Task 4.3: a second `g` while the prefix is armed opens the graph and
        // runs a full solve, mirroring `g v` (design D7).
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some(), "first g arms the prefix");
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);
        assert!(app.prefix.is_none(), "prefix cleared on open");
        let graph = app.graph.as_ref().unwrap();
        assert!(!graph.nodes.is_empty(), "graph holds the patch's circuits");
        assert_eq!(app.graph_positions.len(), graph.nodes.len());
        for (x, y) in &app.graph_positions {
            assert!(x.is_finite() && y.is_finite());
        }
    }

    #[test]
    fn graph_surface_brackets_adjust_cable_tension() {
        // `Alt+[`/`Alt+]` on the focused graph pane lower/raise cable tension
        // and re-solve the layout live; the status reports the current value.
        // Plain brackets now adjust the tiled split instead (task 4.2).
        let mut app = app_with_fixture();
        // Tension is a force-path control: the column arrangement ignores it
        // (graph-column-layout D5), so this test runs the force solver.
        app.layout_mode = crate::config::LayoutMode::Force;
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);
        let default = crate::layout::DEFAULT_TENSION;
        assert_eq!(app.tension, default);
        let before = app.graph_positions.clone();

        handle_event(alt_key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.tension, default + crate::layout::TENSION_STEP);
        assert_eq!(
            app.status_message,
            format!("Cable tension: {:.2}", app.tension)
        );
        assert_ne!(app.graph_positions, before, "tension change re-solves");

        handle_event(alt_key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.tension, default);
        assert_eq!(
            app.graph_positions, before,
            "same tension reproduces layout"
        );
    }

    #[test]
    fn column_mode_tension_adjusts_silently_without_rebuild() {
        // Column arrangement ignores spring stiffness: Alt+[/Alt+] still
        // stores the value (it survives a later switch to force mode) but
        // shows no status and never re-solves (cable-tension spec clause).
        let mut app = app_with_fixture();
        app.layout_mode = crate::config::LayoutMode::Column;
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);
        let default = crate::layout::DEFAULT_TENSION;
        let before = app.graph_positions.clone();
        // The fixture sets `patch` directly, so the starter "No patch loaded"
        // status is stale; clear it so the column-mode silence is observable.
        app.status_message = String::new();

        handle_event(alt_key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.tension, default + crate::layout::TENSION_STEP);
        assert_eq!(app.status_message, String::new(), "no status on column");
        assert_eq!(app.graph_positions, before, "no re-solve on column");
    }

    #[test]
    fn graph_h_toggles_layout_mode_column_to_force_and_resolves() {
        // graph-column-layout 3.1: `h` on the focused graph pane switches the
        // arrangement from the default column path to the force solver. The
        // rebuild re-solves under the new mode and the status names the mode.
        let mut app = app_with_fixture();
        assert_eq!(app.layout_mode, crate::config::LayoutMode::Column);
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);

        handle_event(key(KeyCode::Char('h')), &mut app);

        assert_eq!(app.layout_mode, crate::config::LayoutMode::Force);
        assert_eq!(app.status_message, "Layout: force");
        let graph = app.graph.as_ref().unwrap();
        let expected = crate::layout::solve(graph, &app.pinned_indices(graph), app.tension);
        assert_eq!(
            app.graph_positions, expected,
            "column→force toggle re-solves with the force solver"
        );
    }

    #[test]
    fn graph_h_toggles_layout_mode_force_to_column_and_resolves() {
        // Same toggle in reverse: a force-layout graph flips to the column
        // arrangement, re-solved with the active ordering. The tip stays
        // pinned (design D6): it anchors at its force-solve position while the
        // remaining nodes arrange around it.
        let mut app = app_with_fixture();
        app.layout_mode = crate::config::LayoutMode::Force;
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);
        let before = app.graph_positions.clone();

        handle_event(key(KeyCode::Char('h')), &mut app);

        assert_eq!(app.layout_mode, crate::config::LayoutMode::Column);
        assert_eq!(app.status_message, "Layout: column");
        let graph = app.graph.as_ref().unwrap();
        let anchored: Vec<(usize, (f32, f32))> = app
            .pinned_indices(graph)
            .into_iter()
            .filter_map(|i| before.get(i).map(|&pos| (i, pos)))
            .collect();
        let expected = crate::layout::solve_columns_pinned(
            graph,
            &crate::layout::estimated_widths(graph),
            app.layout_ordering,
            &anchored,
        );
        assert_eq!(
            app.graph_positions, expected,
            "force→column toggle re-solves with the column solver, tip anchored"
        );
    }

    #[test]
    fn zoom_family_plain_and_shift_route_by_focus() {
        // Graph pane focused: plain `+` zooms the camera, `Shift+`+`` scales
        // the module UI/rack instead.
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        seed_graph_camera(&mut app);
        let z0 = app.graph_camera.unwrap().zoom;
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert!((app.graph_camera.unwrap().zoom - z0 * 1.5).abs() < 1e-2);
        let scale_before = app.scale_factor;
        handle_event(shift_key(KeyCode::Char('+')), &mut app);
        assert_ne!(app.scale_factor, scale_before, "other pane scales");
        assert!((app.graph_camera.unwrap().zoom - z0 * 1.5).abs() < 1e-2);
        // Focus off the graph (viewer pane): plain `+` scales the module UI,
        // the camera stays put.
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        let z1 = app.graph_camera.unwrap().zoom;
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert!((app.graph_camera.unwrap().zoom - z1).abs() < 1e-9);
    }

    #[test]
    fn brackets_adjust_main_split_ratio() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        handle_event(key(KeyCode::Char(']')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.6);
        assert_eq!(app.main_split_ratio, 0.6, "mirror stays synced");
        assert_eq!(app.viewer_split_ratio, 0.6, "legacy mirror stays synced");
        assert_eq!(app.status_message, "Split: 60%/40%");
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert_eq!(app.layout.main_split_ratio, 0.5);
    }

    #[test]
    fn backslash_toggles_left_split() {
        let mut app = App::new();
        assert!(!app.left_split_active);
        handle_event(key(KeyCode::Char('\\')), &mut app);
        assert!(app.left_split_active);
        assert_eq!(app.status_message, "Left split on");
        handle_event(key(KeyCode::Char('\\')), &mut app);
        assert!(!app.left_split_active);
        assert_eq!(app.status_message, "Left split off");
    }

    #[test]
    fn viewer_esc_closes_keeping_selection_and_scroll() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        let scroll = app.source_scroll;
        let sel = app.selected_component.clone();
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(!app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        assert_eq!(app.selected_component, sel, "selection kept on close");
        assert_eq!(app.source_scroll, scroll, "scroll kept on close");
        assert!(app.prefix.is_none());
    }

    #[test]
    fn viewer_j_k_scroll_when_source_focused() {
        let mut app = app_with_source_navigation();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, 0);
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.source_scroll, 1);
        handle_event(key(KeyCode::Char('j')), &mut app);
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.source_scroll, 3);
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 2);
        // Saturate at 0
        handle_event(key(KeyCode::Char('k')), &mut app);
        handle_event(key(KeyCode::Char('k')), &mut app);
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 0);
    }

    #[test]
    fn t_toggles_view_mode_when_viewer_open() {
        let mut app = app_with_source_navigation();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
        handle_event(key(KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Prettified);
        handle_event(key(KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    }

    #[test]
    fn t_noop_when_viewer_closed() {
        let mut app = App::new();
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
        handle_event(key(KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    }

    #[test]
    fn tab_switches_focus_when_viewer_open() {
        let mut app = app_with_source_navigation();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
    }

    #[test]
    fn tab_cycles_focus_in_the_startup_layout() {
        // Startup: module UI in the left big pane (focused), source viewer in
        // the first small pane. Tab moves focus to the viewer pane.
        let mut app = App::new();
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert!(app.showing_viewer);
    }

    // ── Focused-pane dispatch (change `pane-class-layout`) ──

    fn shift_tab() -> KeyEvent {
        KeyEvent::new(KeyCode::BackTab, key_modifiers::SHIFT)
    }

    #[test]
    fn tiled_tab_cycles_focus_across_panes() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);
        // The graph replaced the module UI in the left big pane and took focus.
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Graph));
        // Forward: big graph pane -> viewer pane -> back (empty small skipped).
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        // Backward from the big pane lands on the viewer pane.
        handle_event(shift_tab(), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
    }

    #[test]
    fn r_rotates_the_focused_pane_view_within_its_class() {
        // Big pane holding the module UI: `r` cycles the big carousel
        // graph → module UI → graph (module-ui design D1 retires Panels, so
        // BIG_CAROUSEL is [Graph, Physical]). `cycle_view_in_slot` opens the
        // next view in the same pane.
        let mut app = app_with_source_navigation();
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        handle_event(key(KeyCode::Char('r')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Graph));
        assert!(app.showing_graph);
        handle_event(key(KeyCode::Char('r')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        assert!(!app.showing_graph);
        handle_event(key(KeyCode::Char('r')), &mut app);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Graph));
        assert!(app.showing_graph);
        // The pane keeps focus through the rotation.
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
    }

    #[test]
    fn r_keeps_tab_focus_cycling_intact() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app); // focus the viewer (SmallTop)
                               // Small carousel: source viewer -> optimizer in the same pane.
        handle_event(key(KeyCode::Char('r')), &mut app);
        assert_eq!(app.layout.small_top.view, Some(ViewType::Optimizer));
        // Tab still cycles focus: small -> big -> small (empty pane skipped).
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        handle_event(shift_tab(), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
    }

    #[test]
    fn r_on_an_empty_pane_is_a_noop() {
        let mut app = app_with_source_navigation();
        // Focus an empty pane directly: the carousel has no current view.
        app.layout.focus = crate::panes::PaneId::SmallBottom;
        let status = app.status_message.clone();
        handle_event(key(KeyCode::Char('r')), &mut app);
        assert!(app.layout.small_bottom.view.is_none());
        assert_eq!(app.status_message, status);
    }

    #[test]
    fn r_on_optimizer_pane_restores_instead_of_rotating() {
        let mut app = app_with_fixture();
        open_optimizer(&mut app);
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::Optimizer));
        handle_event(key(KeyCode::Char('r')), &mut app);
        // The optimizer pane owns `r` (restore) and returns before the
        // carousel arm; the optimizer is never a carousel member.
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::Optimizer));
        assert!(app.optimizer.is_some());
    }

    #[test]
    fn s_opens_and_focuses_the_physical_pane_and_toggles_skeleton_inside() {
        let mut app = app_with_fixture();
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        // `s` toggles the skeleton inside the already-open physical pane.
        assert!(!app.physical_show_skeleton);
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert!(app.physical_show_skeleton);
        assert_eq!(app.status_message, "Skeleton: on");
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert!(!app.physical_show_skeleton);
        assert_eq!(app.status_message, "Skeleton: off");
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        // With focus moved away, `s` re-focuses the open physical pane.
        handle_event(key(KeyCode::Tab), &mut app); // -> viewer pane
        handle_event(key(KeyCode::Char('s')), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
    }

    #[test]
    fn tiled_esc_closes_focused_view_keeps_others() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph && app.showing_viewer);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        // Graph pane focused: Esc closes it, the viewer survives.
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(!app.showing_graph);
        assert!(app.showing_viewer);
        assert_eq!(app.layout.big_left.view, None, "closed pane is empty");
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
    }

    #[test]
    fn esc_closes_the_focused_view_leaving_the_pane_empty() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        open_viewer(&mut app);
        // Back to the module UI pane: Esc closes it, viewer stays open.
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Physical));
        let sel = app.selected_component.clone();
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.layout.big_left.view, None);
        assert_eq!(app.selected_component, sel, "selection survives a close");
    }

    #[test]
    fn g_q_is_unbound_after_quad_removal() {
        // Quad view is retired with the class layout: `g q` cancels the prefix
        // and leaves the layout untouched.
        let mut app = app_with_source_navigation();
        let slots_before = app.tile_stack.slots.clone();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('q')), &mut app);
        assert!(!app.is_quad());
        assert!(app.prefix.is_none());
        assert_eq!(app.tile_stack.slots, slots_before);
    }

    #[test]
    fn viewer_focus_source_live_panel_keys() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);

        // Shift keys work live while source is focused.
        handle_event(key(KeyCode::Char('1')), &mut app);
        assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
        assert_eq!(app.status_message, "Shift 1 active");

        // Scale preset cycling works live.
        let scale_before = app.scale_factor;
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_ne!(
            app.scale_factor, scale_before,
            "scale live when source focused"
        );

        // Enter toggles the hovered component AND selects it; the selection
        // re-jumps source_scroll to the first occurrence so the visible
        // source view follows the interaction.
        let b11_idx = app
            .patch
            .as_ref()
            .unwrap()
            .hw_components
            .iter()
            .position(|c| c.id == "B1.1")
            .unwrap();
        app.hovered_component = Some(b11_idx);
        let first_b11 = app.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
        let state_before = app.patch.as_ref().unwrap().hw_components[b11_idx]
            .state
            .clone();
        handle_event(key(KeyCode::Enter), &mut app);
        assert_ne!(
            app.patch.as_ref().unwrap().hw_components[b11_idx].state,
            state_before,
            "Enter toggles while source focused"
        );
        assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
        assert_eq!(app.source_scroll, first_b11);
        assert_eq!(app.occurrence_cursor, 0);
        // Space toggles back, still live.
        handle_event(key(KeyCode::Char(' ')), &mut app);
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[b11_idx].state,
            state_before,
            "Space toggles while source focused"
        );

        // j/k and Up/Down/Home/End remain routed by focus (they would
        // otherwise conflict with panel navigation).
        let scroll = app.source_scroll;
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(
            app.source_scroll,
            scroll + 1,
            "j scrolls source when focused"
        );
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, 1, "Down navigates occurrences");
        handle_event(key(KeyCode::Up), &mut app);
        assert_eq!(app.occurrence_cursor, 0, "Up navigates occurrences");
    }

    #[test]
    fn viewer_focus_panels_allows_panel_keys() {
        let mut app = app_with_source_navigation();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        // Switch to panels
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(KeyCode::Char('1')), &mut app);
        assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
        let scale_before = app.scale_factor;
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_ne!(app.scale_factor, scale_before);
        app.hovered_component = Some(0);
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        handle_event(key(KeyCode::Enter), &mut app);
        assert_ne!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before
        );
    }

    #[test]
    fn viewer_occurrence_navigation_up_down_home_end() {
        let mut app = app_with_source_navigation();
        let occurrences = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
        assert!(
            occurrences.len() >= 3,
            "fixture needs at least 3 occurrences"
        );
        app.select_component(String::from("B1.1"));
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // Down -> 1
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, 1);
        assert_eq!(app.source_scroll, occurrences[1].line);
        // Down -> 2
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, 2);
        // saturate at bounds: press Down many times, should end at last
        for _ in 0..10 {
            handle_event(key(KeyCode::Down), &mut app);
        }
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        // Up -> back one
        handle_event(key(KeyCode::Up), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 2);
        // Home -> 0
        handle_event(key(KeyCode::Home), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // End -> last
        handle_event(key(KeyCode::End), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        assert_eq!(app.source_scroll, occurrences.last().unwrap().line);
    }

    #[test]
    fn mouse_click_component_works_when_source_focused() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        // The module UI occupies the left big pane; the source viewer the first
        // small pane. Publish the pane hit rects the renderer would.
        app.pane_hit_rects = vec![
            (crate::panes::PaneId::BigLeft, Rect::new(0, 0, 40, 40)),
            (crate::panes::PaneId::SmallTop, Rect::new(40, 0, 40, 20)),
        ];
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 1),
            &mut app,
        );
        assert_ne!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before,
            "mouse click toggles even when source focused"
        );
        assert_eq!(
            app.viewer_focus,
            ViewerFocus::Panels,
            "component click hands focus to panels"
        );
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
    }

    #[test]
    fn mouse_click_source_pane_space_focuses_source_without_side_effects() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        open_viewer(&mut app);
        // Start from the module UI pane to prove a bare source click switches it.
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        app.pane_hit_rects = vec![
            (crate::panes::PaneId::BigLeft, Rect::new(0, 0, 60, 40)),
            (crate::panes::PaneId::SmallTop, Rect::new(60, 0, 40, 20)),
        ];
        app.minimap_rect = None;
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        let scroll_before = app.source_scroll;
        // Click inside the source pane but on no component and no minimap.
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 80, 10),
            &mut app,
        );
        assert_eq!(
            app.viewer_focus,
            ViewerFocus::Source,
            "bare source-pane click focuses source"
        );
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        assert_eq!(
            app.selected_component.as_deref(),
            Some("B1.1"),
            "selection kept"
        );
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before
        );
        assert_eq!(app.source_scroll, scroll_before);
    }

    #[test]
    fn tiled_mouse_click_component_focuses_panels_slot() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        app.pane_hit_rects = vec![
            (crate::panes::PaneId::BigLeft, Rect::new(0, 0, 40, 40)),
            (crate::panes::PaneId::SmallTop, Rect::new(40, 0, 40, 20)),
        ];
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 1),
            &mut app,
        );
        assert_eq!(
            app.layout.focus,
            crate::panes::PaneId::BigLeft,
            "component click hands focus to the module UI pane"
        );
    }

    #[test]
    fn tiled_mouse_click_source_pane_focuses_source_slot() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        open_viewer(&mut app);
        // Start from the module UI pane to prove a bare source click switches.
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        app.pane_hit_rects = vec![
            (crate::panes::PaneId::BigLeft, Rect::new(0, 0, 60, 40)),
            (crate::panes::PaneId::SmallTop, Rect::new(60, 0, 40, 20)),
        ];
        app.minimap_rect = None;
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 80, 10),
            &mut app,
        );
        assert_eq!(
            app.layout.focus,
            crate::panes::PaneId::SmallTop,
            "source-pane click focuses the source pane"
        );
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
    }

    #[test]
    fn tiled_mouse_click_graph_pane_focuses_graph_slot() {
        // Spec "Mouse click sets focus": a click inside the graph pane focuses
        // it.
        let mut app = app_with_source_navigation();
        app.open_view(ViewType::SourceViewer);
        app.open_graph();
        // The graph replaced the module UI in the left big pane; the viewer
        // open above owns focus, so the click must switch it back.
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Graph));
        // Renderer-published hit rects: graph left, viewer right top.
        app.pane_hit_rects = vec![
            (crate::panes::PaneId::BigLeft, Rect::new(0, 0, 40, 40)),
            (crate::panes::PaneId::SmallTop, Rect::new(40, 0, 120, 20)),
        ];
        // Click inside the graph pane (no node): focus moves to it without
        // starting a drag.
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 5),
            &mut app,
        );
        assert_eq!(
            app.layout.focus,
            crate::panes::PaneId::BigLeft,
            "click inside the graph pane focuses it"
        );
        assert!(app.graph_drag.is_none(), "no node under the click");
    }

    // ---- Task 3.2: selection-into-commit, deselection stability, occurrence bounds ----

    fn idx_for(app: &App, token: &str) -> usize {
        app.patch
            .as_ref()
            .unwrap()
            .hw_components
            .iter()
            .position(|c| c.id == token)
            .unwrap_or_else(|| panic!("no component {token}"))
    }

    #[test]
    fn enter_toggles_and_selects_jumping_to_first_occurrence() {
        let mut app = app_with_source_navigation();
        let token = "B1.1";
        let first = app.patch.as_ref().unwrap().occurrences_for(token)[0].line;
        let idx = idx_for(&app, token);
        app.hovered_component = Some(idx);
        app.source_scroll = 999;
        // Panel focus (viewer closed) -> Enter should toggle + select + jump
        handle_event(key(KeyCode::Enter), &mut app);
        assert_eq!(app.selected_component, Some(String::from(token)));
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, first);
        // Toggled state
        assert!(matches!(
            app.patch.as_ref().unwrap().hw_components[idx].state,
            ComponentState::On
        ));
    }

    #[test]
    fn space_toggles_and_selects_jumping_to_first_occurrence() {
        let mut app = app_with_source_navigation();
        let token = "B1.2";
        let first = app.patch.as_ref().unwrap().occurrences_for(token)[0].line;
        let idx = idx_for(&app, token);
        app.hovered_component = Some(idx);
        handle_event(key(KeyCode::Char(' ')), &mut app);
        assert_eq!(app.selected_component, Some(String::from(token)));
        assert_eq!(app.source_scroll, first);
        assert_eq!(app.occurrence_cursor, 0);
    }

    #[test]
    fn click_toggles_and_selects_jumping_to_first_occurrence() {
        let mut app = app_with_source_navigation();
        let token = "B1.1";
        let first = app.patch.as_ref().unwrap().occurrences_for(token)[0].line;
        let idx = idx_for(&app, token);
        app.component_rects = vec![(idx, Rect::new(10, 10, 16, 2))];
        app.source_scroll = 999;
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 12, 11),
            &mut app,
        );
        assert_eq!(app.selected_component, Some(String::from(token)));
        assert_eq!(app.source_scroll, first);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.hovered_component, Some(idx));
    }

    #[test]
    fn replacement_selection_rejumps_to_new_first_occurrence_via_enter_and_click() {
        let mut app = app_with_source_navigation();
        let b11_first = app.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
        let p11_first = app.patch.as_ref().unwrap().occurrences_for("P1.1")[0].line;
        // First selection via Enter on B1.1
        let b_idx = idx_for(&app, "B1.1");
        app.hovered_component = Some(b_idx);
        handle_event(key(KeyCode::Enter), &mut app);
        assert_eq!(app.source_scroll, b11_first);
        // Replacement via click on P1.1
        let p_idx = idx_for(&app, "P1.1");
        app.component_rects = vec![
            (b_idx, Rect::new(0, 0, 16, 2)),
            (p_idx, Rect::new(20, 0, 16, 2)),
        ];
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 22, 1),
            &mut app,
        );
        assert_eq!(app.selected_component, Some(String::from("P1.1")));
        assert_eq!(app.source_scroll, p11_first);
        assert_eq!(app.occurrence_cursor, 0);
        // And back to B1.1 via Space
        app.hovered_component = Some(b_idx);
        handle_event(key(KeyCode::Char(' ')), &mut app);
        assert_eq!(app.selected_component, Some(String::from("B1.1")));
        assert_eq!(app.source_scroll, b11_first);
    }

    #[test]
    fn empty_panel_click_clears_selection_without_moving_scroll() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        let scroll_before = app.source_scroll;
        assert!(app.selected_component.is_some());
        // Rects only cover component 0 at (0,0); click far away is empty panel space
        let idx0 = idx_for(&app, "B1.1");
        app.component_rects = vec![(idx0, Rect::new(0, 0, 16, 2))];
        // Ensure minimap not interfering
        app.minimap_rect = None;
        // Click empty space
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 100, 50),
            &mut app,
        );
        assert!(app.selected_component.is_none(), "selection cleared");
        assert_eq!(
            app.source_scroll, scroll_before,
            "deselection must not move source_scroll"
        );
        assert_eq!(app.occurrence_cursor, 0);
        // Clicking empty again keeps no-op similarly
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 99, 49),
            &mut app,
        );
        assert!(app.selected_component.is_none());
        assert_eq!(app.source_scroll, scroll_before);
    }

    #[test]
    fn empty_click_on_minimap_does_not_clear_selection() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        let idx0 = idx_for(&app, "B1.1");
        app.component_rects = vec![(idx0, Rect::new(0, 0, 16, 2))];
        app.minimap_rect = Some(Rect::new(70, 0, 10, 20));
        // Click inside the minimap (empty relative to components); the minimap
        // owns the click, so the selection survives.
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 75, 5),
            &mut app,
        );
        assert_eq!(app.selected_component, Some(String::from("B1.1")));
    }

    #[test]
    fn occurrence_navigation_no_selection_noop() {
        let mut app = app_with_source_navigation();
        // Ensure no selection, viewer open and source focused
        assert!(app.selected_component.is_none());
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        app.source_scroll = 5;
        app.occurrence_cursor = 0;
        handle_event(key(KeyCode::Up), &mut app);
        assert_eq!(app.source_scroll, 5);
        assert_eq!(app.occurrence_cursor, 0);
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.source_scroll, 5);
        handle_event(key(KeyCode::Home), &mut app);
        assert_eq!(app.source_scroll, 5);
        handle_event(key(KeyCode::End), &mut app);
        assert_eq!(app.source_scroll, 5);
    }

    #[test]
    fn occurrence_navigation_saturates_at_bounds_via_handler() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        let occurrences = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
        assert!(occurrences.len() >= 2);
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        // Already at first occurrence after select
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // Up at first saturates
        handle_event(key(KeyCode::Up), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // Down to last saturates
        for _ in 0..occurrences.len() + 5 {
            handle_event(key(KeyCode::Down), &mut app);
        }
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        assert_eq!(app.source_scroll, occurrences.last().unwrap().line);
        // Down while at last stays
        handle_event(key(KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        // Home -> first, End -> last
        handle_event(key(KeyCode::Home), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        handle_event(key(KeyCode::End), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
    }

    #[test]
    fn j_k_scroll_remains_when_viewer_open() {
        let mut app = app_with_source_navigation();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, 0);
        handle_event(key(KeyCode::Char('j')), &mut app);
        assert_eq!(app.source_scroll, 1);
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 0);
        // j/k saturate at 0
        handle_event(key(KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 0);
    }

    #[test]
    fn esc_clears_prefix_when_no_view_is_focused() {
        let mut app = App::new();
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.prefix.is_none());
        // When the source viewer is focused, `g` is live too and arms the prefix.
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some(), "g arms even when source focused");
        handle_event(key(KeyCode::Esc), &mut app);
        // Esc clears the prefix first; the viewer stays open.
        assert!(app.showing_viewer);
        assert!(app.prefix.is_none());
        // With the module UI pane focused, `g` arms again and Esc clears it.
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        handle_event(key(KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(app.showing_viewer);
        assert!(app.prefix.is_none());
        // Back to the viewer pane: Esc closes the focused view.
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(!app.showing_viewer);
    }

    // ── Task 4.3: graph handler wiring (`g g`) ──

    /// App with a fixture patch loaded, the graph opened, and node rects
    /// published as the renderer would, so drag hit-testing has geometry.
    fn graph_app() -> App {
        let mut app = app_with_fixture();
        app.open_graph();
        let node_count = app.graph.as_ref().unwrap().nodes.len();
        app.graph_node_rects = (0..node_count)
            .map(|i| (i, Rect::new(10 + (i as u16) * 20, 10, 16, 3)))
            .collect();
        // Tiled renderer contract (change `tiled-window-manager`): mirror a
        // rendered layout so graph events route to graph handling instead of
        // falling through to the panels pane.
        app.pane_rects = vec![
            (FocusSlot::Panels, Rect::new(0, 0, 8, 40)),
            (FocusSlot::Slot(0), Rect::new(8, 0, 192, 40)),
        ];
        app.pane_hit_rects = vec![
            (crate::panes::PaneId::BigLeft, Rect::new(8, 0, 192, 40)),
            (crate::panes::PaneId::SmallTop, Rect::new(0, 0, 8, 40)),
        ];
        app
    }

    #[test]
    fn esc_while_graph_open_closes_and_restores_state() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        app.viewer_focus = ViewerFocus::Source;
        app.source_view_mode = SourceViewMode::Prettified;
        app.source_scroll = 9;
        app.occurrence_cursor = 2;
        let before = (
            app.selected_component.clone(),
            app.viewer_focus.clone(),
            app.source_view_mode.clone(),
            app.source_scroll,
            app.occurrence_cursor,
        );
        app.open_graph();
        assert!(app.showing_graph);
        // `g g` focuses the graph slot; mirror that here so Esc acts on it.
        app.tile_stack.focus = FocusSlot::Slot(0);
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(!app.showing_graph, "Esc closes the graph");
        assert_eq!(app.selected_component, before.0, "selection kept on close");
        assert_eq!(app.viewer_focus, before.1, "viewer focus kept");
        assert_eq!(app.source_view_mode, before.2, "view mode kept");
        assert_eq!(app.source_scroll, before.3, "source scroll kept");
        assert_eq!(app.occurrence_cursor, before.4, "occurrence cursor kept");
        assert!(app.prefix.is_none());
    }

    #[test]
    fn q_quits_while_graph_open() {
        let mut app = app_with_fixture();
        app.open_graph();
        let quit = handle_event(key(KeyCode::Char('q')), &mut app);
        assert!(quit, "q quits even with the graph open");
    }

    #[test]
    fn l_opens_picker_while_graph_open() {
        let mut app = app_with_fixture();
        app.open_graph();
        handle_event(key(KeyCode::Char('l')), &mut app);
        assert!(app.showing_picker, "l opens the picker over the graph");
    }

    #[test]
    fn drag_node_moves_position_resettles_and_emits_node_moved() {
        let mut app = graph_app();
        // The drag re-settle is force-path semantics (local_resettle under
        // spring tension); the column arrangement is now the default, so pin
        // this test to the force arrangement to keep the force-path drag
        // contract covered (graph-column-layout 4.2).
        app.layout_mode = crate::config::LayoutMode::Force;
        assert!(!app.graph.as_ref().unwrap().nodes.is_empty());
        // Subscribe a probe to the synchronous bus to observe NodeMoved.
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let store = std::rc::Rc::clone(&seen);
        app.events
            .subscribe(move |event| store.borrow_mut().push(event.clone()));

        let before = app.graph_positions[0];
        // Down on node 0's rect (0 -> (10,10,16,3)) starts the drag.
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 12, 11),
            &mut app,
        );
        assert!(app.graph_drag.is_some(), "Down on a node rect grabs it");
        // Drag to a new point: position must change and NodeMoved must fire.
        handle_mouse_event(
            mouse(MouseEventKind::Drag(MouseButton::Left), 30, 25),
            &mut app,
        );
        assert_ne!(
            app.graph_positions[0], before,
            "dragged node position changed"
        );
        assert!(
            app.graph_positions
                .iter()
                .all(|(x, y)| x.is_finite() && y.is_finite()),
            "re-settle keeps finite positions"
        );
        assert!(
            seen.borrow()
                .iter()
                .any(|e| matches!(e, Event::NodeMoved(_))),
            "NodeMoved emitted during drag"
        );

        // Up ends the drag; a further Drag must do nothing.
        let after_up = app.graph_positions[0];
        let moves_after_up = seen.borrow().len();
        handle_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Left), 30, 25),
            &mut app,
        );
        assert!(app.graph_drag.is_none(), "Up releases the drag");
        handle_mouse_event(
            mouse(MouseEventKind::Drag(MouseButton::Left), 31, 26),
            &mut app,
        );
        assert_eq!(
            app.graph_positions[0], after_up,
            "post-release drag is a no-op"
        );
        assert_eq!(
            seen.borrow().len(),
            moves_after_up,
            "no NodeMoved after release"
        );
    }

    #[test]
    fn graph_mouse_off_node_rect_is_harmless() {
        let mut app = graph_app();
        let before = app.graph_positions.clone();
        let start_moves = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = std::rc::Rc::clone(&start_moves);
        app.events.subscribe(move |_| count.set(count.get() + 1));

        // Down/Drag/Up entirely off any node rect: no drag starts, no panic,
        // no position change, no events.
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 200, 60),
            &mut app,
        );
        assert!(app.graph_drag.is_none());
        handle_mouse_event(
            mouse(MouseEventKind::Drag(MouseButton::Left), 201, 61),
            &mut app,
        );
        handle_mouse_event(
            mouse(MouseEventKind::Up(MouseButton::Left), 201, 61),
            &mut app,
        );
        assert_eq!(app.graph_positions, before);
        assert_eq!(start_moves.get(), 0, "no events fired off-node");
    }

    #[test]
    fn graph_mouse_down_selects_circuit_and_opens_viewer() {
        let mut app = graph_app();
        let node_id = app.graph.as_ref().unwrap().nodes[0].id.clone();
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 12, 11),
            &mut app,
        );
        assert_eq!(app.selected_circuit(), Some(&node_id));
        assert!(
            app.showing_viewer,
            "selecting a circuit opens the source viewer"
        );
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert!(
            app.tile_stack.is_open(ViewType::SourceViewer),
            "a tiled viewer slot opens so the jump renders"
        );
        // A click on empty space clears hover but keeps the selection.
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 200, 60),
            &mut app,
        );
        assert_eq!(app.selected_circuit(), Some(&node_id));
    }

    #[test]
    fn graph_window_frame_press_selects_circuit() {
        use crate::gui::WindowFrame;
        let mut app = app_with_graph_window();
        let node_id = app.graph.as_ref().unwrap().nodes[0].id.clone();
        let (x, y) = app.graph_positions[0];
        // Window-frame pointers are in window space; the shared camera is
        // pane-relative, so the handler maps them through the pane origin.
        let (ox, oy) = graph_pane_origin(&app);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0 + ox, y + 5.0 + oy)),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.selected_circuit(), Some(&node_id));
        assert!(app.showing_viewer);
        assert!(
            app.tile_stack.is_open(ViewType::SourceViewer),
            "window press opens a tiled viewer slot"
        );
    }

    #[test]
    fn graph_window_minimap_click_pans_camera_to_clicked_world_point() {
        use crate::gui::WindowFrame;
        // Build the published minimap geometry exactly as `minimap_layout`
        // does: panel = (px, py, mw, mh), inner = (px+4, py+4, mw-8, mh-8),
        // sx = inner_w / bw, sy = inner_h / bh, and world = bx + (rel-ix)/sx.
        let mut app = app_with_graph_window();
        let (px, py, mw, mh) = (10.0f32, 100.0f32, 180.0f32, 120.0f32);
        let (bx, by, bw, bh) = (0.0f32, 0.0f32, 1000.0f32, 500.0f32);
        let (ix, iy) = (px + 4.0, py + 4.0);
        let (sx, sy) = ((mw - 8.0) / bw, (mh - 8.0) / bh);
        app.graph_minimap_panel_egui = Some(egui::Rect::from_min_size(
            egui::pos2(px, py),
            egui::vec2(mw, mh),
        ));
        app.graph_minimap_transform = Some((bx, by, bw, bh, ix, iy, sx, sy));
        // Canvas (used for centering) and identity camera.
        app.graph_canvas_px = Some((190.0, 400.0));
        app.graph_camera = Some(crate::graph_render::GraphCamera::new());

        // Click the minimap at the world point (500, 250), forward-mapped to
        // canvas space, then shifted into window space by the pane origin.
        let (ox, oy) = graph_pane_origin(&app);
        let click_x = ix + 500.0 * sx + ox;
        let click_y = iy + 250.0 * sy + oy;
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((click_x, click_y)),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );

        // The camera centers that world point in the published canvas:
        // pan = world * zoom - canvas/2 (zoom 1 here).
        let camera = app.graph_camera.expect("camera stays seeded");
        assert!(
            (camera.pan.0 - (500.0 - 190.0 / 2.0)).abs() < 0.5,
            "pan.x: {}",
            camera.pan.0
        );
        assert!(
            (camera.pan.1 - (250.0 - 400.0 / 2.0)).abs() < 0.5,
            "pan.y: {}",
            camera.pan.1
        );
        assert!(
            app.status_message
                .starts_with("Minimap click: pan to (500, 250)"),
            "status: {}",
            app.status_message
        );
        // A minimap click must not start a node drag or change the selection.
        assert!(app.graph_drag.is_none());
        assert!(app.selected_circuit().is_none());
    }

    #[test]
    fn graph_window_click_outside_minimap_falls_through_to_node_hit() {
        use crate::gui::WindowFrame;
        // A press outside the published minimap panel must reach node
        // hit-testing: the minimap branch returns early only on a real hit.
        let mut app = app_with_graph_window();
        let (px, py, mw, mh) = (10.0f32, 100.0f32, 180.0f32, 120.0f32);
        app.graph_minimap_panel_egui = Some(egui::Rect::from_min_size(
            egui::pos2(px, py),
            egui::vec2(mw, mh),
        ));
        app.graph_minimap_transform = Some((0.0, 0.0, 1000.0, 500.0, 14.0, 104.0, 0.172, 0.224));
        let node_id = app.graph.as_ref().unwrap().nodes[0].id.clone();
        let (x, y) = app.graph_positions[0];
        let (ox, oy) = graph_pane_origin(&app);
        // Node 0 sits near the world origin, far above the bottom-left panel.
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0 + ox, y + 5.0 + oy)),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.selected_circuit(), Some(&node_id));
        assert!(
            app.graph_drag.is_some(),
            "outside the minimap a node drag starts"
        );
    }

    #[test]
    fn graph_window_frame_marquee_selects_primary_and_opens_viewer() {
        use crate::gui::{MarqueeSelection, WindowFrame};
        let mut app = app_with_graph_window();
        let nodes = &app.graph.as_ref().unwrap().nodes;
        assert!(nodes.len() >= 2, "fixture needs two nodes for a marquee");
        let primary = nodes[1].id.clone();
        handle_graph_window_frame(
            &WindowFrame {
                marquee: Some(MarqueeSelection {
                    rect: (0.0, 0.0, 400.0, 200.0),
                    nodes: vec![1, 0],
                }),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.selected_circuit(), Some(&primary));
        assert!(app.showing_viewer);
        assert!(
            app.tile_stack.is_open(ViewType::SourceViewer),
            "marquee commit opens a tiled viewer slot"
        );
    }

    #[test]
    fn graph_window_frame_marquee_repeat_is_idempotent() {
        use crate::gui::{MarqueeSelection, WindowFrame};
        let mut app = app_with_graph_window();
        let frame = WindowFrame {
            marquee: Some(MarqueeSelection {
                rect: (0.0, 0.0, 400.0, 200.0),
                nodes: vec![1, 0],
            }),
            ..Default::default()
        };
        handle_graph_window_frame(&frame, &mut app);
        // A held drag must not churn viewer state: perturb the committed
        // scroll/cursor, then repeat the identical frame.
        app.source_scroll = 9999;
        app.occurrence_cursor = 7;
        let selected = app.selected_circuit().cloned();
        handle_graph_window_frame(&frame, &mut app);
        assert_eq!(app.selected_circuit(), selected.as_ref());
        assert_eq!(app.source_scroll, 9999);
        assert_eq!(app.occurrence_cursor, 7);
    }

    #[test]
    fn graph_window_frame_empty_marquee_leaves_selection() {
        use crate::gui::{MarqueeSelection, WindowFrame};
        let mut app = app_with_graph_window();
        assert_eq!(app.selected_circuit(), None);
        handle_graph_window_frame(
            &WindowFrame {
                marquee: Some(MarqueeSelection {
                    rect: (0.0, 0.0, 400.0, 200.0),
                    nodes: Vec::new(),
                }),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.selected_circuit(), None);
        assert_eq!(app.selected_circuit(), None);
    }

    // ── 5.1 regression anchoring inside handler.rs (fixtures/source_navigation.ini) ──
    // Each test below drives real flows end-to-end through handle_event/handle_mouse_event + render
    // so they break if geometry, prefix, or viewer routing drifts. The dedicated
    // src/regression.rs holds the full suite; these smoke tests anchor the
    // same coverage directly in handler.rs per task 5.1 scope requirement.
    #[test]
    fn regression_handler_e2e_initial_bof_and_selected_open() {
        let mut app = app_with_source_navigation();
        app.source_scroll = 77;
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, 0, "BOF when no selection");
        // selected-open jumps to first occurrence
        let mut app2 = app_with_source_navigation();
        let first = app2.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
        app2.select_component(String::from("B1.1"));
        app2.source_scroll = 999;
        app2.showing_viewer = false;
        handle_event(key(KeyCode::Char('g')), &mut app2);
        handle_event(key(KeyCode::Char('v')), &mut app2);
        assert_eq!(app2.source_scroll, first);
        assert_eq!(app2.occurrence_cursor, 0);
    }

    #[test]
    fn regression_handler_e2e_t_and_tab_and_picker_and_isolation() {
        let mut app = app_with_source_navigation();
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        // t preserves usable content: toggles but stays in bounds
        let scroll_before = app.source_scroll;
        handle_event(key(KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, crate::app::SourceViewMode::Prettified);
        assert_eq!(app.source_scroll, scroll_before);
        handle_event(key(KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, crate::app::SourceViewMode::Raw);
        // Tab round-trip Source->Panels->Source
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        // picker precedence: l opens picker even when source focused
        handle_event(key(KeyCode::Char('l')), &mut app);
        assert!(app.showing_picker, "picker overlays viewer");
        // while picker open, t is inert
        let mode_before = app.source_view_mode.clone();
        handle_event(key(KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, mode_before);
        handle_event(key(KeyCode::Esc), &mut app);
        assert!(!app.showing_picker);
        assert!(app.showing_viewer);
        // live interaction: panel keys work even when Source focused
        if app.viewer_focus != ViewerFocus::Source {
            handle_event(key(KeyCode::Tab), &mut app);
        }
        let scale_before = app.scale_factor;
        handle_event(key(KeyCode::Char('+')), &mut app);
        assert_ne!(
            app.scale_factor, scale_before,
            "scale live when Source focused"
        );
        handle_event(key(KeyCode::Char('1')), &mut app);
        assert_eq!(
            app.active_shift,
            Some(ShiftGroup::Group1),
            "shift live when Source focused"
        );
    }

    // ── Task 2.1: global processing pause (`p`) ──

    #[test]
    fn p_toggles_processing_pause_with_status() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(app.processing_paused);
        assert_eq!(app.status_message, "Processing paused (p to resume)");
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(!app.processing_paused);
        assert_eq!(app.status_message, "Processing enabled (p to pause)");
    }

    #[test]
    fn p_toggles_pin_on_graph_surface() {
        let mut app = app_with_fixture();
        app.open_graph();
        let node_count = app.graph.as_ref().unwrap().nodes.len();
        assert!(node_count >= 2, "fixture needs a non-tip node to toggle");
        let idx = 1; // non-tip node: the tip is pinned by default
        app.hovered_graph_node = Some(idx);
        let node = app.graph.as_ref().unwrap().nodes[idx].clone();
        assert!(
            !app.pinned.contains(&node.id),
            "non-tip node starts unpinned"
        );
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(app.pinned.contains(&node.id), "p pins the hovered node");
        assert!(app.showing_graph, "p must not close the graph");
        assert!(
            !app.processing_paused,
            "p no longer toggles pause on the graph surface"
        );
        assert_eq!(
            app.status_message,
            format!("Pinned: {} {}", node.circuit, node.instance_index)
        );
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(!app.pinned.contains(&node.id), "second p unpins");
        assert_eq!(
            app.status_message,
            format!("Unpinned: {} {}", node.circuit, node.instance_index)
        );
    }

    #[test]
    fn p_noop_while_picker_open() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(!app.processing_paused, "picker swallows p");
    }

    #[test]
    fn enter_and_space_do_not_mutate_while_paused() {
        let mut app = app_with_fixture();
        app.hovered_component = Some(0);
        handle_event(key(KeyCode::Char('p')), &mut app);
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        handle_event(key(KeyCode::Enter), &mut app);
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before,
            "Enter must not toggle while paused"
        );
        handle_event(key(KeyCode::Char(' ')), &mut app);
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before,
            "Space must not toggle while paused"
        );
        // Selection still works while paused.
        assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
    }

    #[test]
    fn mouse_click_toggle_blocked_while_paused() {
        let mut app = app_with_fixture();
        handle_event(key(KeyCode::Char('p')), &mut app);
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 1),
            &mut app,
        );
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before,
            "mouse toggle blocked while paused"
        );
        // Hover and selection keep working.
        assert_eq!(app.hovered_component, Some(0));
        assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
    }

    #[test]
    fn scroll_adjustment_blocked_while_paused() {
        let content = "[pot]\n    pot = P1.1\n    output = _X\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];
        handle_event(key(KeyCode::Char('p')), &mut app);
        handle_mouse_event(mouse(MouseEventKind::ScrollUp, 5, 1), &mut app);
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => assert!(v.abs() < 1e-6, "scroll blocked while paused"),
            _ => panic!("expected Value state"),
        }
        handle_mouse_event(mouse(MouseEventKind::ScrollDown, 5, 1), &mut app);
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => assert!(v.abs() < 1e-6, "scroll blocked while paused"),
            _ => panic!("expected Value state"),
        }
    }

    #[test]
    fn p_toggles_pause_while_viewer_open_source_focused() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(app.processing_paused, "p live in the source pane");
        assert_eq!(app.status_message, "Processing paused (p to resume)");
    }

    fn app_with_graph() -> App {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.open_graph();
        assert!(app.showing_graph);
        // Simulate renderer publishing node rects so hover hit-testing would work;
        // for keyboard `x` tests we set hovered_graph_node directly, but populate
        // rects anyway for completeness.
        let node_count = app.graph.as_ref().unwrap().nodes.len();
        app.graph_node_rects = (0..node_count)
            .map(|i| (i, Rect::new(i as u16 * 22, 0, 22, 5)))
            .collect();
        // Tiled renderer contract (change `tiled-window-manager`): publish
        // the graph slot rect so mouse events route to graph handling.
        app.pane_rects = vec![
            (FocusSlot::Panels, Rect::new(0, 0, 8, 40)),
            (FocusSlot::Slot(0), Rect::new(8, 0, 192, 40)),
        ];
        app.pane_hit_rects = vec![
            (crate::panes::PaneId::BigLeft, Rect::new(8, 0, 192, 40)),
            (crate::panes::PaneId::SmallTop, Rect::new(0, 0, 8, 40)),
        ];
        app
    }

    #[test]
    fn graph_x_disables_hovered_node_and_rebuilds() {
        let mut app = app_with_graph();
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        app.hovered_graph_node = Some(0);
        let before_positions = app.graph_positions.clone();
        handle_event(key(KeyCode::Char('x')), &mut app);
        assert!(app.showing_graph, "x must not close the graph surface");
        assert!(
            app.disabled_circuits
                .contains(&NodeId::circuit(&node.circuit, node.instance_index)),
            "hovered circuit should be disabled"
        );
        assert_eq!(
            app.status_message,
            format!(
                "Processing disabled: {} {}",
                node.circuit, node.instance_index
            )
        );
        assert!(app.graph.is_some(), "graph rebuilt after toggle");
        assert_eq!(
            app.graph.as_ref().unwrap().nodes.len(),
            before_positions.len(),
            "node count preserved after rebuild"
        );
    }

    #[test]
    fn graph_x_second_press_reenables_hovered_node() {
        let mut app = app_with_graph();
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        app.hovered_graph_node = Some(0);
        handle_event(key(KeyCode::Char('x')), &mut app);
        assert!(app
            .disabled_circuits
            .contains(&NodeId::circuit(&node.circuit, node.instance_index)));
        // Second x on same hovered node re-enables.
        handle_event(key(KeyCode::Char('x')), &mut app);
        assert!(
            !app.disabled_circuits
                .contains(&NodeId::circuit(&node.circuit, node.instance_index)),
            "second x re-enables"
        );
        assert_eq!(
            app.status_message,
            format!(
                "Processing enabled: {} {}",
                node.circuit, node.instance_index
            )
        );
        assert!(app.showing_graph);
    }

    #[test]
    fn graph_x_no_hover_is_silent_noop() {
        let mut app = app_with_graph();
        app.hovered_graph_node = None;
        let status_before = app.status_message.clone();
        let disabled_before = app.disabled_circuits.clone();
        let positions_before = app.graph_positions.clone();
        handle_event(key(KeyCode::Char('x')), &mut app);
        assert_eq!(
            app.status_message, status_before,
            "no status change when nothing hovered"
        );
        assert_eq!(app.disabled_circuits, disabled_before);
        assert_eq!(
            app.graph_positions, positions_before,
            "no rebuild when no hover"
        );
        assert!(app.showing_graph);
    }

    #[test]
    fn graph_p_toggles_pin_not_pause() {
        let mut app = app_with_graph();
        assert!(!app.processing_paused);
        let node_count = app.graph.as_ref().unwrap().nodes.len();
        assert!(node_count >= 2, "fixture needs a non-tip node to toggle");
        let idx = node_count - 1; // non-tip node
        app.hovered_graph_node = Some(idx);
        let node = app.graph.as_ref().unwrap().nodes[idx].clone();
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(app.pinned.contains(&node.id), "p pins the hovered node");
        assert_eq!(
            app.status_message,
            format!("Pinned: {} {}", node.circuit, node.instance_index)
        );
        assert!(app.showing_graph, "p must not close the graph");
        assert!(
            !app.processing_paused,
            "p no longer toggles pause on the graph surface"
        );
        handle_event(key(KeyCode::Char('p')), &mut app);
        assert!(!app.pinned.contains(&node.id), "second p unpins");
        assert!(!app.processing_paused, "pause untouched throughout");
    }

    // --- GPU graph window interaction mapping (task 3.1) --------------------
    // The window hit-tests pointers against the graph layout via the camera;
    // with an identity camera (zoom 1, pan 0) world == pixel, so pointers can
    // be taken straight from `graph_positions`.

    fn app_with_graph_window() -> App {
        let mut app = app_with_graph();
        app.graph_camera = Some(crate::graph_render::GraphCamera::new());
        app
    }

    /// Pointer just inside `target`'s drawn world box, accepted only when no
    /// earlier node's box also claims it (hit-testing takes the first match),
    /// so the sample deterministically hits `target`.
    fn clean_sample_point(app: &App, target: usize) -> Option<(f32, f32)> {
        let world = node_world_sizes_of(app);
        let (px, py) = app.graph_positions[target];
        let (sx, sy) = (px + 5.0, py + 5.0);
        let (tw, th) = world[target];
        let claims = |i: usize| {
            let (nx, ny) = app.graph_positions[i];
            let (w, h) = world[i];
            let _ = (tw, th);
            sx >= nx && sx < nx + w && sy >= ny && sy < ny + h
        };
        (claims(target) && !(0..target).any(claims)).then_some((sx, sy))
    }

    /// Per-node world extents of the app's current graph (empty without one).
    fn node_world_sizes_of(app: &App) -> Vec<(f32, f32)> {
        app.graph
            .as_ref()
            .map(crate::layout::node_world_sizes)
            .unwrap_or_default()
    }

    #[test]
    fn graph_window_hover_sets_and_clears_hovered_node() {
        use crate::gui::WindowFrame;
        let mut app = app_with_graph_window();
        let (x, y) = app.graph_positions[0];
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 10.0, y + 10.0)),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.hovered_graph_node, Some(0));
        // A pointer beyond every node's drawn right edge misses all boxes.
        let world = node_world_sizes_of(&app);
        let max_right = app
            .graph_positions
            .iter()
            .enumerate()
            .map(|(i, &(nx, _))| nx + world[i].0)
            .fold(f32::MIN, f32::max);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((max_right + 50.0, 0.0)),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.hovered_graph_node, None);
        // Leaving the window clears hover too.
        handle_graph_window_frame(&WindowFrame::default(), &mut app);
        assert_eq!(app.hovered_graph_node, None);
    }

    #[test]
    fn graph_window_hit_test_uses_world_extents_at_zoom_2() {
        // At zoom 2 the minimum hit size does not bind: a click inside the
        // drawn frame selects the node, a click in the gap selects nothing.
        use crate::gui::WindowFrame;
        let mut app = app_with_graph();
        let (ox, oy) = graph_pane_origin(&app);
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 2.0,
            pan: (0.0, 0.0),
        });
        app.graph_canvas_px = Some((960.0, 480.0));
        let world = node_world_sizes_of(&app);
        let (x, y) = app.graph_positions[0];
        let (w, h) = world[0];
        // World point just inside the frame → pixel = world * 2, plus the pane
        // origin for the window-space pointer.
        let (px, py) = ((x + w / 2.0) * 2.0 + ox, (y + h / 2.0) * 2.0 + oy);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((px, py)),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.hovered_graph_node, Some(0));
        // A point half a node height below the box is outside every drawn
        // frame: the world height is 80 > GRAPH_MIN_HIT_PX / zoom (6), so no
        // node claims it.
        let gap_y = ((y + h + 20.0) * 2.0) + oy;
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((px, gap_y)),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(
            app.hovered_graph_node, None,
            "a click in the gap between frames selects nothing"
        );
    }

    #[test]
    fn graph_window_hit_test_minimum_size_at_zoom_0_1() {
        // At zoom 0.1 a node's drawn frame is only a few pixels, so the minimum
        // hit size (`GRAPH_MIN_HIT_PX / zoom` in world units) must still make a
        // click near the node's center land on it.
        use crate::gui::WindowFrame;
        let mut app = app_with_graph();
        let (ox, oy) = graph_pane_origin(&app);
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 0.1,
            pan: (0.0, 0.0),
        });
        app.graph_canvas_px = Some((960.0, 480.0));
        let world = node_world_sizes_of(&app);
        let (x, y) = app.graph_positions[0];
        let (w, h) = world[0];
        // World point at the node center; with zoom 0.1 the drawn frame is
        // w * 0.1 × h * 0.1 pixels. The minimum hit rect is GRAPH_MIN_HIT_PX
        // (=12) pixels wide in world units = 120 world units, so even a point
        // offset beyond the drawn frame hits centroid proximity. Test the exact
        // center here: it must land, and the node must be selected on press.
        let (wx, wy) = (x + w / 2.0, y + h / 2.0);
        let (px, py) = (wx * 0.1 + ox, wy * 0.1 + oy);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((px, py)),
                primary_pressed: true,
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.hovered_graph_node, Some(0));
        let expected = app.graph.as_ref().unwrap().nodes[0].id.clone();
        assert_eq!(app.selected_circuit(), Some(&expected));
        // Sanity: the world hit rect is min_h / zoom across — far larger than
        // the drawn frame, which is why a low-zoom click still lands.
        assert!(App::GRAPH_MIN_HIT_PX / 0.1 > w);
    }

    #[test]
    fn graph_window_drag_moves_node_and_auto_pins_on_release() {
        use crate::gui::WindowFrame;
        let mut app = app_with_graph_window();
        let (x, y) = app.graph_positions[0];
        let node_id = app.graph.as_ref().unwrap().nodes[0].id.clone();
        // Window-frame pointers are in window space; the shared camera is
        // pane-relative, so the handler maps them through the pane origin.
        let (ox, oy) = graph_pane_origin(&app);
        // Press on node 0: grabs the node, no move yet (mirrors the terminal
        // Down/Drag split).
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0 + ox, y + 5.0 + oy)),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(app.hovered_graph_node, Some(0));
        let drag = app.graph_drag.as_ref().expect("press grabs the node");
        assert_eq!(drag.node_index, 0);
        // Drag the pointer +30/+20: the node follows without jumping.
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 35.0 + ox, y + 25.0 + oy)),
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        let (nx, ny) = app.graph_positions[0];
        assert!((nx - (x + 30.0)).abs() < 1e-3, "node x follows the drag");
        assert!((ny - (y + 20.0)).abs() < 1e-3, "node y follows the drag");
        // Release auto-pins the dropped node (design D7).
        handle_graph_window_frame(
            &WindowFrame {
                primary_released: true,
                ..Default::default()
            },
            &mut app,
        );
        assert!(app.graph_drag.is_none());
        assert!(app.pinned.contains(&node_id));
    }

    #[test]
    fn graph_window_drag_emits_node_moved_event() {
        use crate::gui::WindowFrame;
        let mut app = app_with_graph_window();
        let received = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let store = std::rc::Rc::clone(&received);
        app.events.subscribe(move |event| {
            if let Event::NodeMoved(_) = event {
                store.borrow_mut().push(event.clone());
            }
        });
        let (ox, oy) = graph_pane_origin(&app);
        let (x, y) = app.graph_positions[0];
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0 + ox, y + 5.0 + oy)),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 35.0 + ox, y + 25.0 + oy)),
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(received.borrow().len(), 1, "one NodeMoved per drag frame");
        assert!(matches!(received.borrow()[0], Event::NodeMoved(_)));
    }

    #[test]
    fn graph_window_keys_act_on_hovered_node() {
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        let (x, y) = app.graph_positions[0];
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        let (ox, oy) = graph_pane_origin(&app);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0 + ox, y + 5.0 + oy)),
                keys: vec![WindowGraphKey::ToggleProcessing],
                ..Default::default()
            },
            &mut app,
        );
        assert!(
            app.disabled_circuits
                .contains(&NodeId::circuit(&node.circuit, node.instance_index)),
            "window x disables the hovered circuit"
        );
        assert_eq!(
            app.status_message,
            format!(
                "Processing disabled: {} {}",
                node.circuit, node.instance_index
            )
        );
    }

    // --- GPU graph window camera keys (design D3/D4) -----------------------
    // Mirrors of the terminal `c`/`Shift+c` tests: the window dispatches the
    // same camera mutations through `WindowGraphKey`.

    #[test]
    fn graph_window_c_key_centers_camera() {
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        // Seed an off-center camera + canvas, as the renderer publishes them.
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 2.0,
            pan: (10.0, 20.0),
        });
        app.graph_canvas_px = Some((640.0, 300.0));

        handle_graph_window_frame(
            &WindowFrame {
                keys: vec![WindowGraphKey::CenterGraph],
                ..Default::default()
            },
            &mut app,
        );

        let cam = app.graph_camera.unwrap();
        assert_eq!(cam.zoom, 2.0, "centering must not change zoom");
        assert_ne!(cam.pan, (10.0, 20.0), "centering must pan");
        assert_eq!(app.status_message, "Graph centered");
    }

    #[test]
    fn graph_window_c_centering_is_noop_without_canvas_size() {
        // Mirror of the app.rs contract: without a published canvas size the
        // center is a silent no-op — no pan, no status change.
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 2.0,
            pan: (10.0, 20.0),
        });
        assert!(app.graph_canvas_px.is_none());

        handle_graph_window_frame(
            &WindowFrame {
                keys: vec![WindowGraphKey::CenterGraph],
                ..Default::default()
            },
            &mut app,
        );

        assert_eq!(
            app.graph_camera.unwrap().pan,
            (10.0, 20.0),
            "no-op pan without canvas size"
        );
        assert_ne!(app.status_message, "Graph centered");
    }

    #[test]
    fn graph_window_shift_c_refits_camera_against_published_canvas() {
        // Design D3/D4: Shift+c is a full refit against the published pane
        // size, and the zoom preset resets to the fitted-zoom entry.
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 9.0,
            pan: (123.0, 456.0),
        });
        app.graph_zoom_preset = 0;
        app.graph_canvas_px = Some((640.0, 300.0));

        handle_graph_window_frame(
            &WindowFrame {
                keys: vec![WindowGraphKey::FitGraph],
                ..Default::default()
            },
            &mut app,
        );

        assert_eq!(app.graph_zoom_preset, App::GRAPH_ZOOM_FIT_INDEX as u8);
        assert_eq!(app.graph_canvas_px, Some((640.0, 300.0)));
        assert!(
            app.status_message.contains("Graph zoom"),
            "unexpected status: {:?}",
            app.status_message
        );
        let cam = app.graph_camera.unwrap();
        assert!(cam.zoom > 0.0 && cam.zoom.is_finite(), "refit zooms");
    }

    #[test]
    fn graph_window_shift_c_falls_back_to_default_viewport_before_first_publish() {
        // Before the renderer publishes a canvas size the refit uses the
        // default viewport and publishes it, so later keys anchor on it.
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        app.graph_camera = Some(crate::graph_render::GraphCamera {
            zoom: 9.0,
            pan: (123.0, 456.0),
        });
        assert!(app.graph_canvas_px.is_none());

        handle_graph_window_frame(
            &WindowFrame {
                keys: vec![WindowGraphKey::FitGraph],
                ..Default::default()
            },
            &mut app,
        );

        assert_eq!(app.graph_canvas_px, Some((1280.0, 800.0)));
        assert_eq!(app.graph_zoom_preset, App::GRAPH_ZOOM_FIT_INDEX as u8);
    }

    #[test]
    fn graph_window_pin_key_toggles_pin_on_hovered_node() {
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        let node_count = app.graph.as_ref().unwrap().nodes.len();
        assert!(node_count >= 2, "fixture needs a non-tip node to toggle");
        // Pick the highest non-tip node with a sample point no earlier node
        // overlaps, so the hit deterministically lands on it.
        let (idx, point) = (1..node_count)
            .rev()
            .find_map(|i| clean_sample_point(&app, i).map(|p| (i, p)))
            .expect("fixture has a clean non-tip sample point");
        let node = app.graph.as_ref().unwrap().nodes[idx].clone();
        // `clean_sample_point` is world space; window-frame pointers are
        // window space, offset by the pane origin.
        let (ox, oy) = graph_pane_origin(&app);
        let point = (point.0 + ox, point.1 + oy);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some(point),
                keys: vec![WindowGraphKey::TogglePin],
                ..Default::default()
            },
            &mut app,
        );
        assert!(
            app.pinned.contains(&node.id),
            "window p pins the hovered node"
        );
        assert_eq!(
            app.status_message,
            format!("Pinned: {} {}", node.circuit, node.instance_index)
        );
    }

    #[test]
    fn graph_window_edit_key_opens_circuit_overlay() {
        use crate::app::EditKind;
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        let (x, y) = app.graph_positions[0];
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        let (ox, oy) = graph_pane_origin(&app);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0 + ox, y + 5.0 + oy)),
                keys: vec![WindowGraphKey::BeginEdit],
                ..Default::default()
            },
            &mut app,
        );
        let editing = app.editing.as_ref().expect("e opens the edit overlay");
        assert_eq!(editing.kind, EditKind::Circuit { node: node.id });
    }

    #[test]
    fn graph_window_pan_and_zoom_reach_shared_camera() {
        use crate::gui::{camera_pan, camera_zoom_about, WindowFrame};
        let mut app = app_with_graph_window();
        let before = app.graph_camera.unwrap();
        handle_graph_window_frame(
            &WindowFrame {
                pan_delta: (10.0, -5.0),
                ..Default::default()
            },
            &mut app,
        );
        let panned = camera_pan(&before, 10.0, -5.0);
        assert_eq!(
            app.graph_camera.unwrap(),
            panned,
            "window pan reaches the shared camera"
        );
        let anchor = (100.0, 100.0);
        // The wheel-zoom anchor arrives in window coordinates; the handler
        // maps it through the pane origin before reaching the shared camera.
        let (ox, oy) = graph_pane_origin(&app);
        handle_graph_window_frame(
            &WindowFrame {
                zoom: Some((1.5, anchor)),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(
            app.graph_camera.unwrap(),
            camera_zoom_about(&panned, 1.5, (anchor.0 - ox, anchor.1 - oy)),
            "window wheel zoom reaches the shared camera"
        );
    }

    #[test]
    fn graph_window_without_camera_is_inert() {
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph(); // no camera
        app.hovered_graph_node = Some(0);
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((5.0, 5.0)),
                primary_pressed: true,
                keys: vec![WindowGraphKey::ToggleProcessing],
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(
            app.hovered_graph_node, None,
            "hover cleared when no camera can hit-test"
        );
        assert!(app.graph_drag.is_none());
        assert!(app.disabled_circuits.is_empty());
    }

    #[test]
    fn drag_auto_pins_the_dropped_node() {
        let mut app = app_with_graph();
        let idx = 1;
        let (_, rect) = app
            .graph_node_rects
            .iter()
            .find(|(i, _)| *i == idx)
            .map(|(i, r)| (*i, *r))
            .unwrap();
        // Down on the node, drag by (+7, +1), then release.
        handle_mouse_event(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                rect.x + 1,
                rect.y + 1,
            ),
            &mut app,
        );
        handle_mouse_event(
            mouse(
                MouseEventKind::Drag(MouseButton::Left),
                rect.x + 8,
                rect.y + 2,
            ),
            &mut app,
        );
        handle_mouse_event(
            mouse(
                MouseEventKind::Up(MouseButton::Left),
                rect.x + 8,
                rect.y + 2,
            ),
            &mut app,
        );
        let node = app.graph.as_ref().unwrap().nodes[idx].clone();
        assert!(
            app.pinned.contains(&node.id),
            "drag auto-pins the dropped node"
        );
        // The anchor stays put: a subsequent drag of another node must not
        // move the pinned node.
        let other_idx = 2;
        let (_, other_rect) = app
            .graph_node_rects
            .iter()
            .find(|(i, _)| *i == other_idx)
            .map(|(i, r)| (*i, *r))
            .unwrap();
        let pinned_pos = app.graph_positions[idx];
        handle_mouse_event(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                other_rect.x + 1,
                other_rect.y + 1,
            ),
            &mut app,
        );
        handle_mouse_event(
            mouse(
                MouseEventKind::Drag(MouseButton::Left),
                other_rect.x + 4,
                other_rect.y + 2,
            ),
            &mut app,
        );
        handle_mouse_event(
            mouse(
                MouseEventKind::Up(MouseButton::Left),
                other_rect.x + 4,
                other_rect.y + 2,
            ),
            &mut app,
        );
        assert_eq!(
            app.graph_positions[idx], pinned_pos,
            "pinned anchor stays put"
        );
    }

    #[test]
    fn picker_f_toggles_favourite_and_back_preserving_highlight() {
        use tempfile::TempDir;
        let _env_guard = fav_lock().lock().unwrap();
        let xdg_dir = TempDir::new().unwrap();
        let orig_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", xdg_dir.path());
        // Isolate picker dir with a real .ini file on disk.
        let picker_tmp = TempDir::new().unwrap();
        let dummy = picker_tmp.path().join("my_patch.ini");
        std::fs::write(&dummy, "[p2b8]\nbutton1 = B1.1\n").unwrap();
        // A second file so the picker list has more than one entry.
        let other = picker_tmp.path().join("other.ini");
        std::fs::write(&other, "[p2b8]\nbutton1 = B1.2\n").unwrap();

        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        app.picker_dir = picker_tmp.path().to_path_buf();
        app.showing_picker = true;
        app.refresh_picker_entries();
        let dummy_idx = app
            .picker_entries
            .iter()
            .position(|p| {
                crate::favorites::FavoritesStore::canonical_key(p)
                    == crate::favorites::FavoritesStore::canonical_key(&dummy)
            })
            .expect("dummy must be in picker");
        app.picker_index = dummy_idx;
        assert!(
            !app.favorites.is_favourite(&dummy),
            "precondition: not favourited"
        );

        // First press f -> mark favourite.
        let target_key = crate::favorites::FavoritesStore::canonical_key(&dummy);
        handle_picker_event(key(KeyCode::Char('f')), &mut app);
        assert!(
            app.favorites.is_favourite(&dummy),
            "f should mark favourite"
        );
        assert!(
            app.status_message.contains("Favourited"),
            "status: {:?}",
            app.status_message
        );
        // Highlight must follow the entry even after refresh re-sorts pinned section.
        let pos = app
            .picker_entries
            .iter()
            .position(|p| crate::favorites::FavoritesStore::canonical_key(p) == target_key)
            .unwrap();
        assert_eq!(
            app.picker_index, pos,
            "highlight preserved via canonical_key"
        );
        // Persisted file exists under the isolated XDG dir.
        let fav_file = xdg_dir.path().join("droid-tui").join("favourites.toml");
        assert!(fav_file.exists(), "save() should write favourites.toml");
        let body = std::fs::read_to_string(&fav_file).unwrap();
        assert!(body.contains("my_patch.ini"), "body: {body}");

        // Second press f -> unmark favourite (toggle back).
        handle_picker_event(key(KeyCode::Char('f')), &mut app);
        assert!(
            !app.favorites.is_favourite(&dummy),
            "second f should unmark favourite"
        );
        assert!(
            app.status_message.contains("Unfavourited"),
            "status: {:?}",
            app.status_message
        );
        let pos2 = app
            .picker_entries
            .iter()
            .position(|p| crate::favorites::FavoritesStore::canonical_key(p) == target_key)
            .unwrap();
        assert_eq!(
            app.picker_index, pos2,
            "highlight still preserved after unmark"
        );

        // Uppercase F behaves identically (case-insensitive toggle key).
        handle_picker_event(key(KeyCode::Char('F')), &mut app);
        assert!(
            app.favorites.is_favourite(&dummy),
            "F should also mark favourite"
        );
        handle_picker_event(key(KeyCode::Char('F')), &mut app);
        assert!(
            !app.favorites.is_favourite(&dummy),
            "second F should unmark"
        );

        // Restore env.
        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn picker_f_ignores_parent_dir_and_ctrl_modifier() {
        use tempfile::TempDir;
        let _env_guard = fav_lock().lock().unwrap();
        let xdg_dir = TempDir::new().unwrap();
        let orig_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", xdg_dir.path());

        let picker_tmp = TempDir::new().unwrap();
        let dummy = picker_tmp.path().join("a.ini");
        std::fs::write(&dummy, "[p2b8]\nbutton1 = B1.1\n").unwrap();

        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        app.picker_dir = picker_tmp.path().to_path_buf();
        app.showing_picker = true;
        app.refresh_picker_entries();

        // Parent ".." sentinel is always first when picker_dir has a parent.
        assert!(
            is_picker_parent_entry(&app.picker_entries[0]),
            "first entry should be .."
        );
        app.picker_index = 0;
        handle_picker_event(key(KeyCode::Char('f')), &mut app);
        assert!(
            app.favorites.favourites.is_empty(),
            "f on parent must not toggle"
        );

        // Ctrl+f must be ignored even on a file entry.
        let file_idx = app
            .picker_entries
            .iter()
            .position(|p| {
                crate::favorites::FavoritesStore::canonical_key(p)
                    == crate::favorites::FavoritesStore::canonical_key(&dummy)
            })
            .unwrap();
        app.picker_index = file_idx;
        let ctrl_f = KeyEvent::new(KeyCode::Char('f'), key_modifiers::CONTROL);
        handle_picker_event(ctrl_f, &mut app);
        assert!(
            app.favorites.favourites.is_empty(),
            "Ctrl+f must not toggle"
        );
        assert!(
            !app.favorites.is_favourite(&dummy),
            "file still not favourited"
        );

        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn picker_favourite_integration_loads_via_enter() {
        use tempfile::TempDir;
        let _env_guard = fav_lock().lock().unwrap();
        let xdg_dir = TempDir::new().unwrap();
        let orig_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", xdg_dir.path());

        let picker_tmp = TempDir::new().unwrap();
        let fav_path = picker_tmp.path().join("alpha.ini");
        let other_path = picker_tmp.path().join("beta.ini");
        std::fs::write(&fav_path, "[p2b8]\nbutton = B1.1\n").unwrap();
        std::fs::write(&other_path, "[p2b8]\nbutton = B1.2\n").unwrap();

        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        // Favourite alpha via FavoritesStore (canonicalized like the real toggle path).
        assert!(app.favorites.toggle(&fav_path));
        assert!(app.favorites.is_favourite(&fav_path));
        assert!(!app.favorites.is_favourite(&other_path));

        app.picker_dir = picker_tmp.path().to_path_buf();
        app.showing_picker = true;
        app.picker_index = 0;
        app.refresh_picker_entries();

        // Favourite must be pinned at index 0, ahead of parent sentinel and directory listing.
        assert!(!app.picker_entries.is_empty(), "picker should have entries");
        let first = &app.picker_entries[0];
        assert_eq!(
            crate::favorites::FavoritesStore::canonical_key(first),
            crate::favorites::FavoritesStore::canonical_key(&fav_path),
            "favourite should be at index 0, got {:?}",
            first
        );
        assert!(
            app.is_favourite_entry(first),
            "is_favourite_entry should be true for pinned favourite"
        );
        let label = app.picker_entry_label(first);
        assert!(
            label.contains('★'),
            "favourited label should contain ★, got {label:?}"
        );
        assert!(
            label.contains("alpha.ini"),
            "label should contain leaf name, got {label:?}"
        );
        // Other file remains in listing but not at top; parent ".." follows favourites.
        let other_pos = app
            .picker_entries
            .iter()
            .position(|p| {
                crate::favorites::FavoritesStore::canonical_key(p)
                    == crate::favorites::FavoritesStore::canonical_key(&other_path)
            })
            .expect("other.ini should be in picker");
        assert!(
            other_pos > 0,
            "other.ini should not be at top (pos {other_pos})"
        );
        assert!(
            !app.is_favourite_entry(&other_path),
            "other.ini should not be favourited"
        );
        assert!(
            !app.picker_entry_label(&other_path).contains('★'),
            "non-favourite label should not contain ★"
        );

        // Simulate Enter on the favourited entry to load the patch.
        app.picker_index = 0;
        let quit = handle_picker_event(key(KeyCode::Enter), &mut app);
        assert!(!quit, "Enter should not quit");
        assert!(
            !app.showing_picker,
            "picker should close after successful load"
        );
        assert!(app.patch.is_some(), "patch should be loaded");
        let patch = app.patch.as_ref().unwrap();
        assert!(
            patch.hw_components.iter().any(|c| c.id == "B1.1"),
            "loaded fav patch should contain B1.1"
        );
        assert!(app.selected_file.is_some(), "selected_file should be set");
        assert_eq!(
            crate::favorites::FavoritesStore::canonical_key(app.selected_file.as_ref().unwrap()),
            crate::favorites::FavoritesStore::canonical_key(&fav_path)
        );
        assert!(
            app.status_message.contains("Loaded") || app.status_message.contains("Ready"),
            "status should indicate load success, got {:?}",
            app.status_message
        );

        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn picker_f_toggles_directory_favourite_and_back() {
        use tempfile::TempDir;
        let _env_guard = fav_lock().lock().unwrap();
        let xdg_dir = TempDir::new().unwrap();
        let orig_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", xdg_dir.path());

        let picker_tmp = TempDir::new().unwrap();
        let subdir = picker_tmp.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        let other = picker_tmp.path().join("other.ini");
        std::fs::write(&other, "[p2b8]\nbutton1 = B1.2\n").unwrap();

        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        app.picker_dir = picker_tmp.path().to_path_buf();
        app.showing_picker = true;
        app.refresh_picker_entries();
        let dir_idx = app
            .picker_entries
            .iter()
            .position(|p| p == &subdir)
            .expect("subdir in picker");
        app.picker_index = dir_idx;
        assert!(!app.favorites.is_favourite(&subdir));

        // First press f -> mark directory favourite.
        handle_picker_event(key(KeyCode::Char('f')), &mut app);
        assert!(
            app.favorites.is_favourite(&subdir),
            "f should favourite a directory"
        );
        assert!(
            app.status_message.contains("Favourited"),
            "status: {:?}",
            app.status_message
        );
        // Directory is pinned at the top; highlight follows the re-sorted list.
        let target_key = crate::favorites::FavoritesStore::canonical_key(&subdir);
        let pos = app
            .picker_entries
            .iter()
            .position(|p| crate::favorites::FavoritesStore::canonical_key(p) == target_key)
            .unwrap();
        assert_eq!(app.picker_index, pos, "highlight follows the pinned entry");

        // Second press f -> unmark.
        handle_picker_event(key(KeyCode::Char('f')), &mut app);
        assert!(
            !app.favorites.is_favourite(&subdir),
            "second f should unfavourite a directory"
        );
        assert!(
            app.status_message.contains("Unfavourited"),
            "status: {:?}",
            app.status_message
        );

        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn picker_digit_on_file_favourite_loads_and_closes() {
        use tempfile::TempDir;
        let _env_guard = fav_lock().lock().unwrap();
        let xdg_dir = TempDir::new().unwrap();
        let orig_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", xdg_dir.path());

        let picker_tmp = TempDir::new().unwrap();
        let alpha = picker_tmp.path().join("alpha.ini");
        std::fs::write(&alpha, "[p2b8]\nbutton = B1.1\n").unwrap();
        let beta = picker_tmp.path().join("beta.ini");
        std::fs::write(&beta, "[p2b8]\nbutton = B1.2\n").unwrap();

        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        app.favorites.toggle(&alpha);
        app.favorites.toggle(&beta);
        app.picker_dir = picker_tmp.path().to_path_buf();
        app.showing_picker = true;
        app.refresh_picker_entries();

        // Slot order follows the sorted favourites list: alpha.ini (0), beta.ini (1).
        let favs = app.picker_entries_with_favourites();
        assert_eq!(favs.len(), 2);
        assert!(favs[0].ends_with("alpha.ini"));
        assert!(favs[1].ends_with("beta.ini"));

        // '1' opens favourite slot 1: patch loads, picker closes.
        app.picker_index = 0;
        handle_picker_event(key(KeyCode::Char('1')), &mut app);
        assert!(!app.showing_picker, "digit on file favourite closes picker");
        assert!(app.patch.is_some(), "digit loads the favourite patch");
        assert!(
            app.selected_file
                .as_ref()
                .is_some_and(|p| p.ends_with("beta.ini")),
            "selected_file: {:?}",
            app.selected_file
        );

        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn picker_digit_on_directory_favourite_navigates_in() {
        use tempfile::TempDir;
        let _env_guard = fav_lock().lock().unwrap();
        let xdg_dir = TempDir::new().unwrap();
        let orig_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", xdg_dir.path());

        let picker_tmp = TempDir::new().unwrap();
        let subdir = picker_tmp.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("inner.ini"), "[p2b8]\nbutton = B1.1\n").unwrap();

        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        app.favorites.toggle(&subdir);
        app.picker_dir = picker_tmp.path().to_path_buf();
        app.showing_picker = true;
        app.refresh_picker_entries();

        // '0' on slot 0 (the directory favourite) navigates into it.
        handle_picker_event(key(KeyCode::Char('0')), &mut app);
        assert!(app.showing_picker, "digit on directory keeps picker open");
        assert!(app.picker_dir.ends_with("subdir"));
        assert!(app.patch.is_none());

        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn picker_digit_out_of_range_and_modifiers_are_noop() {
        use tempfile::TempDir;
        let _env_guard = fav_lock().lock().unwrap();
        let xdg_dir = TempDir::new().unwrap();
        let orig_xdg = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", xdg_dir.path());

        let picker_tmp = TempDir::new().unwrap();
        let alpha = picker_tmp.path().join("alpha.ini");
        std::fs::write(&alpha, "[p2b8]\nbutton = B1.1\n").unwrap();

        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        app.favorites.toggle(&alpha);
        app.picker_dir = picker_tmp.path().to_path_buf();
        app.showing_picker = true;
        app.refresh_picker_entries();

        // '9' has no slot: silent no-op, picker stays open.
        handle_picker_event(key(KeyCode::Char('9')), &mut app);
        assert!(app.showing_picker);
        assert!(app.patch.is_none());
        assert!(app.selected_file.is_none());

        // Ctrl/Alt-held digits never fire (mirrors the f-handler guard).
        let ctrl_zero = KeyEvent::new(KeyCode::Char('0'), key_modifiers::CONTROL);
        handle_picker_event(ctrl_zero, &mut app);
        assert!(app.showing_picker, "Ctrl+digit must not open a favourite");
        assert!(app.patch.is_none());
        let alt_zero = KeyEvent::new(KeyCode::Char('0'), key_modifiers::ALT);
        handle_picker_event(alt_zero, &mut app);
        assert!(app.showing_picker, "Alt+digit must not open a favourite");
        assert!(app.patch.is_none());

        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    // ==== Window keyboard path (task 2.6) ====
    //
    // winit key events convert to the neutral shape and run the same dispatch
    // as the terminal key path. These mirror the terminal tests through
    // `handle_window_key_event` so every binding keeps its meaning in the
    // window: shift groups, g-prefix, p/x, scale/split, s/o, Tab focus, Esc.

    use winit::event::ElementState;
    use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};

    fn winit_key(named: NamedKey) -> WinitKey {
        WinitKey::Named(named)
    }

    fn winit_char(ch: &str) -> WinitKey {
        WinitKey::Character(ch.into())
    }

    fn window_press(key: &WinitKey, modifiers: ModifiersState, app: &mut App) -> bool {
        handle_window_key_event(key, ElementState::Pressed, modifiers, app)
    }

    #[test]
    fn from_winit_maps_character_and_named_keys() {
        let mapped = |k: &WinitKey, m: ModifiersState| {
            KeyEvent::from_winit_parts(k, ElementState::Pressed, m)
        };
        // Produced characters: shifted glyphs arrive as their symbol.
        assert_eq!(
            mapped(&winit_char("g"), ModifiersState::empty())
                .unwrap()
                .code,
            KeyCode::Char('g')
        );
        assert_eq!(
            mapped(&winit_char("G"), ModifiersState::SHIFT)
                .unwrap()
                .code,
            KeyCode::Char('G')
        );
        assert_eq!(
            mapped(&winit_char("?"), ModifiersState::SHIFT)
                .unwrap()
                .code,
            KeyCode::Char('?')
        );
        assert_eq!(
            mapped(&winit_char("+"), ModifiersState::SHIFT)
                .unwrap()
                .code,
            KeyCode::Char('+')
        );
        // Named keys map to the matching code; Shift+Tab becomes BackTab.
        for (named, code) in [
            (NamedKey::Enter, KeyCode::Enter),
            (NamedKey::Escape, KeyCode::Esc),
            (NamedKey::Backspace, KeyCode::Backspace),
            (NamedKey::Tab, KeyCode::Tab),
            (NamedKey::Space, KeyCode::Char(' ')),
            (NamedKey::ArrowUp, KeyCode::Up),
            (NamedKey::ArrowDown, KeyCode::Down),
            (NamedKey::ArrowLeft, KeyCode::Left),
            (NamedKey::ArrowRight, KeyCode::Right),
            (NamedKey::Home, KeyCode::Home),
            (NamedKey::End, KeyCode::End),
        ] {
            assert_eq!(
                mapped(&winit_key(named), ModifiersState::empty())
                    .unwrap()
                    .code,
                code
            );
        }
        assert_eq!(
            mapped(&winit_key(NamedKey::Tab), ModifiersState::SHIFT)
                .unwrap()
                .code,
            KeyCode::BackTab
        );
        // Control maps to ctrl+command (the handler treats the pair as one).
        let ctrl = mapped(&winit_char("c"), ModifiersState::CONTROL).unwrap();
        assert_eq!(ctrl.code, KeyCode::Char('c'));
        assert!(ctrl.modifiers.ctrl && ctrl.modifiers.command);
    }

    #[test]
    fn from_winit_filters_release_multi_char_and_unbound() {
        assert!(KeyEvent::from_winit_parts(
            &winit_char("g"),
            ElementState::Released,
            ModifiersState::empty()
        )
        .is_none());
        // Multi-character composition (IME/dead key) is not a binding.
        assert!(KeyEvent::from_winit_parts(
            &winit_char("ab"),
            ElementState::Pressed,
            ModifiersState::empty()
        )
        .is_none());
        // Unbound named keys and unidentified keys map to None.
        assert!(KeyEvent::from_winit_parts(
            &winit_key(NamedKey::PageDown),
            ElementState::Pressed,
            ModifiersState::empty()
        )
        .is_none());
        assert!(KeyEvent::from_winit_parts(
            &winit_key(NamedKey::Delete),
            ElementState::Pressed,
            ModifiersState::empty()
        )
        .is_none());
        assert!(KeyEvent::from_winit_parts(
            &WinitKey::Unidentified(winit::keyboard::NativeKey::Unidentified),
            ElementState::Pressed,
            ModifiersState::empty()
        )
        .is_none());
    }

    #[test]
    fn window_shift_groups_1_through_4() {
        let mut app = App::new();
        for (ch, expected) in [
            ('1', ShiftGroup::Group1),
            ('2', ShiftGroup::Group2),
            ('3', ShiftGroup::Group3),
            ('4', ShiftGroup::Group4),
        ] {
            assert!(!window_press(
                &winit_char(&ch.to_string()),
                ModifiersState::empty(),
                &mut app
            ));
            assert_eq!(app.active_shift, Some(expected));
        }
        assert!(!window_press(
            &winit_key(NamedKey::Escape),
            ModifiersState::empty(),
            &mut app
        ));
        assert_eq!(app.active_shift, None);
    }

    #[test]
    fn window_g_prefix_opens_graph_tile() {
        let mut app = app_with_fixture();
        assert!(!window_press(
            &winit_char("g"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.prefix.is_some(), "first g arms the prefix");
        assert!(!window_press(
            &winit_char("g"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.showing_graph);
        assert!(app.prefix.is_none(), "prefix cleared on open");
        let graph = app.graph.as_ref().unwrap();
        assert!(!graph.nodes.is_empty(), "graph holds the patch's circuits");
        assert_eq!(app.graph_positions.len(), graph.nodes.len());
    }

    #[test]
    fn window_question_mark_opens_help() {
        let mut app = App::new();
        assert!(!window_press(
            &winit_char("?"),
            ModifiersState::SHIFT,
            &mut app
        ));
        assert!(app.showing_help);
    }

    #[test]
    fn window_plus_minus_cycle_scale() {
        let mut app = App::new();
        assert!(!window_press(
            &winit_char("-"),
            ModifiersState::empty(),
            &mut app
        ));
        assert_eq!(app.scale_factor, 0.75);
        assert_eq!(app.status_message, "Scaling: 75%");
        assert!(!window_press(
            &winit_char("+"),
            ModifiersState::empty(),
            &mut app
        ));
        assert_eq!(app.scale_factor, 1.0);
        assert_eq!(app.status_message, "Scaling: 100%");
        // Shift+plus is the other-pane route (no-op on the panels surface),
        // mirroring the terminal `+`/`-` handler.
        assert!(!window_press(
            &winit_char("+"),
            ModifiersState::SHIFT,
            &mut app
        ));
        assert_eq!(app.scale_factor, 1.0);
    }

    #[test]
    fn window_p_x_toggle_hovered_graph_node() {
        // `x` disables processing on the hovered node (mirrors the terminal
        // graph-surface test, on a fresh app because the rebuild reindexes).
        let mut app = app_with_graph();
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        app.hovered_graph_node = Some(0);
        assert!(!window_press(
            &winit_char("x"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(
            app.disabled_circuits
                .contains(&NodeId::circuit(&node.circuit, node.instance_index)),
            "hovered circuit should be disabled"
        );
        // `p` pins the hovered node (mirrors `graph_p_toggles_pin_not_pause`,
        // using the non-tip node so the pin is a real anchor).
        let mut app = app_with_graph();
        let node_count = app.graph.as_ref().unwrap().nodes.len();
        assert!(node_count >= 2, "fixture needs a non-tip node to toggle");
        let idx = node_count - 1;
        app.hovered_graph_node = Some(idx);
        let node = app.graph.as_ref().unwrap().nodes[idx].clone();
        assert!(!window_press(
            &winit_char("p"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(
            app.pinned.contains(&node.id),
            "hovered node should be pinned"
        );
        assert!(
            !app.processing_paused,
            "p must not pause on the graph surface"
        );
    }

    #[test]
    fn window_ctrl_c_quits() {
        let mut app = App::new();
        assert!(window_press(
            &winit_char("c"),
            ModifiersState::CONTROL,
            &mut app
        ));
    }

    #[test]
    fn window_shift_tab_cycles_focus_backward() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert!(!window_press(
            &winit_char("g"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(!window_press(
            &winit_char("g"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.showing_graph);
        // `g g` focuses the graph pane in the left big slot.
        assert_eq!(app.layout.focus, crate::panes::PaneId::BigLeft);
        // Shift+Tab cycles backward to the viewer pane.
        assert!(!window_press(
            &winit_key(NamedKey::Tab),
            ModifiersState::SHIFT,
            &mut app
        ));
        assert_eq!(app.layout.focus, crate::panes::PaneId::SmallTop);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
    }

    #[test]
    fn window_esc_closes_help() {
        let mut app = App::new();
        window_press(&winit_char("?"), ModifiersState::SHIFT, &mut app);
        assert!(app.showing_help);
        let quit = window_press(
            &winit_key(NamedKey::Escape),
            ModifiersState::empty(),
            &mut app,
        );
        assert!(!quit);
        assert!(!app.showing_help);
    }

    #[test]
    fn window_unbound_keys_are_ignored() {
        let mut app = App::new();
        let before = (app.scale_factor, app.active_shift, app.showing_picker);
        assert!(!window_press(
            &winit_key(NamedKey::PageDown),
            ModifiersState::empty(),
            &mut app
        ));
        assert_eq!(
            (app.scale_factor, app.active_shift, app.showing_picker),
            before,
            "unbound key must not mutate app state"
        );
    }

    #[test]
    fn window_non_keyboard_events_do_not_quit() {
        // Close/redraw/modifier handling lives in main.rs's event loop, never
        // the keyboard quit path; each non-keyboard variant must hit the
        // `_ => false` arm of `handle_window_event`.
        let cases = [
            WindowEvent::CloseRequested,
            WindowEvent::RedrawRequested,
            WindowEvent::ModifiersChanged(winit::event::Modifiers::default()),
        ];
        for event in cases {
            let mut app = App::new();
            assert!(
                !handle_window_event(&event, ModifiersState::empty(), &mut app),
                "non-keyboard window event must not set the quit flag"
            );
        }
    }

    #[test]
    fn window_q_quits() {
        let mut app = App::new();
        assert!(window_press(
            &winit_char("q"),
            ModifiersState::empty(),
            &mut app
        ));
        // A plain `q` press quits and leaves the app unmutated otherwise
        // (the startup source viewer stays open).
        assert!(!app.showing_picker);
        assert!(app.showing_viewer);
        assert!(app.prefix.is_none());
        assert_eq!(app.active_shift, None);
    }

    #[test]
    fn window_l_opens_picker() {
        let mut app = app_with_fixture();
        assert!(!window_press(
            &winit_char("l"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.showing_picker, "l opens the file picker");
    }

    #[test]
    fn window_g_v_opens_source_viewer() {
        let mut app = app_with_fixture();
        assert!(!window_press(
            &winit_char("g"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.prefix.is_some(), "first g arms the prefix");
        assert!(!window_press(
            &winit_char("v"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.showing_viewer, "g v opens the source viewer");
        assert!(
            app.tile_stack.is_open(ViewType::SourceViewer),
            "the viewer takes a right-column slot"
        );
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert!(app.prefix.is_none(), "prefix cleared on open");
    }

    #[test]
    fn window_e_opens_label_edit_overlay() {
        let mut app = app_with_fixture();
        app.hovered_component = Some(0);
        assert!(!window_press(
            &winit_char("e"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(
            app.editing.is_some(),
            "e opens the label edit overlay on the hovered component"
        );
    }

    #[test]
    fn window_esc_clears_armed_prefix() {
        let mut app = app_with_fixture();
        assert!(!window_press(
            &winit_char("g"),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.prefix.is_some());
        assert!(!window_press(
            &winit_key(NamedKey::Escape),
            ModifiersState::empty(),
            &mut app
        ));
        assert!(app.prefix.is_none(), "Esc cancels the armed prefix");
    }

    #[test]
    fn panels_frame_hover_sets_hovered_component() {
        let mut app = app_with_fixture();
        handle_panels_frame(
            PanelsFrame {
                hovered: Some(1),
                ..PanelsFrame::default()
            },
            &mut app,
        );
        assert_eq!(app.hovered_component, Some(1));
    }

    #[test]
    fn panels_frame_click_toggles_and_selects() {
        let mut app = app_with_fixture();
        assert!(matches!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            ComponentState::Off
        ));
        handle_panels_frame(
            PanelsFrame {
                clicked: Some(0),
                ..PanelsFrame::default()
            },
            &mut app,
        );
        assert!(matches!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            ComponentState::On
        ));
        assert_eq!(
            app.status_message,
            format!(
                "Toggled: {}",
                app.patch.as_ref().unwrap().hw_components[0].label
            )
        );
        assert_eq!(app.tile_stack.focus, FocusSlot::Panels);
    }

    #[test]
    fn panels_frame_scroll_adjusts_knob_value() {
        let content = "[pot]\n    pot = P1.1\n    output = _X\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];

        let expected_delta = 1.0 * ZOOM_SENSITIVITY * 10.0;
        handle_panels_frame(
            PanelsFrame {
                hovered: Some(0),
                scroll: Some(1.0),
                ..PanelsFrame::default()
            },
            &mut app,
        );
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => assert!((v - expected_delta).abs() < 1e-6),
            _ => panic!("expected Value state"),
        }
    }

    #[test]
    fn physical_frame_toggles_skeleton_and_applies_pan_zoom() {
        let mut app = App::new();
        handle_physical_frame(
            PhysicalFrame {
                skeleton_toggle: true,
                ..PhysicalFrame::default()
            },
            &mut app,
        );
        assert!(app.physical_show_skeleton);
        assert_eq!(app.status_message, "Skeleton: on");

        handle_physical_frame(
            PhysicalFrame {
                pan_delta: (2.0, -3.0),
                ..PhysicalFrame::default()
            },
            &mut app,
        );
        assert_eq!(app.physical_offset, (2.0, -3.0));

        let before = app.physical_zoom;
        handle_physical_frame(
            PhysicalFrame {
                zoom: Some((2.0, (5.0, 5.0))),
                ..PhysicalFrame::default()
            },
            &mut app,
        );
        // The frame clamps the factor by MAX_ZOOM_STEP before applying it.
        let factor = 2.0_f32.clamp(1.0 / MAX_ZOOM_STEP, MAX_ZOOM_STEP);
        assert!((app.physical_zoom - (before * factor).clamp(0.5, 3.0)).abs() < 1e-6);
        assert_eq!(app.scale_factor, app.physical_zoom);
    }

    #[test]
    fn viewer_frame_scroll_moves_source_scroll() {
        let mut app = app_with_source_navigation();
        handle_viewer_frame(ViewerFrame { scroll_delta: 3.0 }, &mut app);
        assert_eq!(app.source_scroll, 3);
        handle_viewer_frame(ViewerFrame { scroll_delta: -2.0 }, &mut app);
        assert_eq!(app.source_scroll, 1);
    }

    #[test]
    fn picker_frame_hover_and_click_map_to_app() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_picker_frame(
            PickerFrame {
                hovered: Some(1),
                clicked: None,
            },
            &mut app,
        );
        assert_eq!(app.picker_index, 1);
        // Clicking the patch_a.ini entry loads it and closes the picker.
        let idx = picker_index_of(&app, "patch_a.ini");
        handle_picker_frame(
            PickerFrame {
                hovered: None,
                clicked: Some(idx),
            },
            &mut app,
        );
        assert!(!app.showing_picker);
        assert_eq!(app.patch.as_ref().unwrap().name, "patch_a");
    }
}
