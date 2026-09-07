use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(feature = "gui")]
use crate::app::GraphWindowRequest;
use crate::app::{
    is_entry_selectable, is_picker_parent_entry, App, FocusSlot, GraphDrag, PrefixState,
    SourceViewMode, ViewType, ViewerFocus,
};
use crate::layout;
use crate::patch::{ComponentKind, ComponentState, HwComponent, NodeId, Patch, ShiftGroup};

/// How long an armed `g` prefix waits for its follow-up key before silently
/// cancelling. The timeout is lazy: it is checked only when the next event
/// arrives, so no timer thread or event-loop change is needed.
const PREFIX_TIMEOUT: Duration = Duration::from_secs(1);

/// Compat mirror: `viewer_focus` follows tile focus until handler/ui migrate
/// fully (task 6.1 removes `viewer_focus`). Source-nav gating still reads
/// `viewer_focus`, so every tile focus change re-derives it.
fn sync_viewer_focus_from_tiles(app: &mut App) {
    app.viewer_focus = match app.tile_stack.focus {
        FocusSlot::Slot(i) if app.tile_stack.slots.get(i) == Some(&ViewType::SourceViewer) => {
            ViewerFocus::Source
        }
        _ => ViewerFocus::Panels,
    };
}

/// True when keys should act on the graph pane: the graph slot holds tile
/// focus, or no tiles exist yet (legacy flag-owned graph surface).
fn graph_slot_focused(app: &App) -> bool {
    match app.tile_stack.focus {
        FocusSlot::Slot(i) => app.tile_stack.slots.get(i) == Some(&ViewType::Graph),
        FocusSlot::Panels => app.tile_stack.slots.is_empty(),
    }
}

/// True when keys should act on the optimizer pane: the optimizer slot holds
/// tile focus. No legacy flag path exists — the optimizer was a modal overlay
/// until task 5.1 — so Panels focus never routes here.
fn optimizer_slot_focused(app: &App) -> bool {
    match app.tile_stack.focus {
        FocusSlot::Slot(i) => app.tile_stack.slots.get(i) == Some(&ViewType::Optimizer),
        FocusSlot::Panels => false,
    }
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

/// Focus a right-column slot holding `view` (no-op when not open).
fn focus_tile_slot(app: &mut App, view: ViewType) {
    if let Some(i) = app.tile_stack.slots.iter().position(|v| *v == view) {
        app.tile_stack.focus = FocusSlot::Slot(i);
    }
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
            crossterm::event::KeyCode::Esc => {
                app.cancel_edit();
                app.status_message = String::from("Edit cancelled");
                return false;
            }
            crossterm::event::KeyCode::Enter => {
                match app.commit_edit() {
                    Ok(()) => app.status_message = String::from("Label saved"),
                    Err(e) => app.status_message = format!("Save failed: {e}"),
                }
                return false;
            }
            crossterm::event::KeyCode::Backspace => {
                if let Some(state) = app.editing.as_mut() {
                    state.draft.pop();
                }
                return false;
            }
            crossterm::event::KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
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
            crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('q') => {
                app.close_help();
                return false;
            }
            _ => return false,
        }
    }
    // `?` opens the help modal from any view (design D2). Matched without a
    // modifier guard because the key arrives with SHIFT (Shift+/), the same
    // convention as `+` (Shift+=). The edit overlay above already eats it.
    if matches!(key.code, crossterm::event::KeyCode::Char('?')) {
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
            crossterm::event::KeyCode::Esc => {
                app.showing_validation = false;
                return false;
            }
            crossterm::event::KeyCode::Char('e') if key.modifiers.is_empty() => {
                app.showing_validation = false;
                return false;
            }
            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                if app.validation_cursor + 1 < app.validation_issues.len() {
                    app.validation_cursor += 1;
                }
                return false;
            }
            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                if app.validation_cursor > 0 {
                    app.validation_cursor -= 1;
                }
                return false;
            }
            crossterm::event::KeyCode::Enter => {
                if let Some(issue) = app.validation_issues.get(app.validation_cursor).cloned() {
                    app.source_scroll = issue.span.line;
                    // Open source viewer and focus it so the jumped span is visible.
                    app.showing_viewer = true;
                    app.viewer_focus = ViewerFocus::Source;
                    app.tile_stack.open(ViewType::SourceViewer);
                    focus_tile_slot(app, ViewType::SourceViewer);
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
    if matches!(key.code, crossterm::event::KeyCode::Char('e'))
        && key.modifiers.is_empty()
        && !app.validation_issues.is_empty()
        && !app.showing_validation
    {
        let has_label_target = app.hovered_graph_node.is_some()
            || app.hovered_component.is_some()
            || (app.showing_viewer && app.viewer_focus == ViewerFocus::Source);
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
            crossterm::event::KeyCode::Char('v') => {
                open_embedded_viewer(app);
                // Tiled open path (change `tiled-window-manager`, D7): the
                // viewer takes a right-column slot and focus with it.
                app.tile_stack.open(ViewType::SourceViewer);
                focus_tile_slot(app, ViewType::SourceViewer);
                return false;
            }
            crossterm::event::KeyCode::Char('g') => {
                // `g g` opens the graph surface, mirroring `g v` (design D7).
                #[cfg(feature = "gui")]
                if app.graph_window_enabled {
                    // `[gui] graph_window = true`: open the GPU graph window
                    // instead of the terminal tile (gpu-graph-window D6); the
                    // windowed loop in main.rs consumes the request next frame.
                    app.request_graph_window(GraphWindowRequest::Open);
                    app.prefix = None;
                    return false;
                }
                app.open_graph();
                // Tiled open path: `open_graph` registers the slot; focus
                // follows it so Esc/keys act on the graph pane.
                focus_tile_slot(app, ViewType::Graph);
                sync_viewer_focus_from_tiles(app);
                app.prefix = None;
                return false;
            }
            crossterm::event::KeyCode::Char('d') => {
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
            crossterm::event::KeyCode::Char('o') => {
                // `g o` opens the optimizer as a right-column pane (change
                // `tiled-window-manager`, 5.1): `open_view` handles the
                // already-open focus case and the slot cap, and lands focus
                // on the pane.
                app.open_view(ViewType::Optimizer);
                sync_viewer_focus_from_tiles(app);
                app.prefix = None;
                return false;
            }
            crossterm::event::KeyCode::Char('w') => {
                // `g w` toggles the GPU graph window (gpu-graph-window D6).
                // The handler cannot reach GraphWindow (owned by the windowed
                // loop in main.rs), so it records a request the loop consumes
                // on its next frame; without the `gui` feature the key shows a
                // status hint and does nothing else (keybinding spec).
                #[cfg(feature = "gui")]
                {
                    app.request_graph_window(GraphWindowRequest::Toggle);
                }
                #[cfg(not(feature = "gui"))]
                {
                    app.status_message =
                        String::from("GPU graph window requires the `gui` feature");
                }
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

    // Diff overlay Esc handling: clear scope first, then overlay (before viewer/graph Esc).
    if matches!(key.code, crossterm::event::KeyCode::Esc) {
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

    // Focused-pane dispatch (change `tiled-window-manager`, D7): while
    // right-column slots are open, `Tab` cycles tile focus forward,
    // `Shift+Tab`/`BackTab` cycles backward, and `Esc` closes the focused
    // view (or clears the modifier selection on panels). Empty-stack states
    // fall through to the legacy per-view branches below.
    if !app.tile_stack.slots.is_empty() {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            crossterm::event::KeyCode::Tab if !shift => {
                app.cycle_focus(true);
                sync_viewer_focus_from_tiles(app);
                return false;
            }
            crossterm::event::KeyCode::Tab | crossterm::event::KeyCode::BackTab => {
                app.cycle_focus(false);
                sync_viewer_focus_from_tiles(app);
                return false;
            }
            crossterm::event::KeyCode::Esc => {
                let closed_viewer = matches!(
                    app.tile_stack.focus,
                    FocusSlot::Slot(i)
                        if app.tile_stack.slots.get(i) == Some(&ViewType::SourceViewer)
                );
                app.close_focused_view();
                // Only a viewer close resets the legacy focus mirror; other
                // views (graph, optimizer) never owned it.
                if closed_viewer {
                    app.viewer_focus = ViewerFocus::Panels;
                }
                app.prefix = None;
                return false;
            }
            _ => {}
        }
    }

    // Optimizer pane (change `tiled-window-manager`, 5.1): the optimizer is
    // a right-column view, so its keys respond only while the optimizer slot
    // holds focus (mirroring the graph/viewer pane dispatch). Esc already
    // closed the slot above via `close_focused_view`; unhandled keys fall
    // through so q/Ctrl+C quit and `l` still opens the picker.
    if optimizer_slot_focused(app) {
        match key.code {
            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
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
            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                if let Some(state) = app.optimizer.as_mut() {
                    if state.cursor > 0 {
                        state.cursor -= 1;
                    }
                }
                return false;
            }
            crossterm::event::KeyCode::Enter => {
                let idx = app.optimizer.as_ref().map(|s| s.cursor).unwrap_or(0);
                app.optimizer_preview(idx);
                return false;
            }
            crossterm::event::KeyCode::Char('r') => {
                app.optimizer_restore();
                return false;
            }
            crossterm::event::KeyCode::Char('s') => {
                let idx = app.optimizer.as_ref().map(|s| s.cursor).unwrap_or(0);
                app.optimizer_export(idx);
                return false;
            }
            // Weight slider (design D5): `[`/`]` step ±0.1 in [0,1], `0`/`1`
            // snap to the endpoints. Returned here so the split-ratio
            // handlers never see them while the optimizer pane is focused.
            crossterm::event::KeyCode::Char('[') => {
                if let Some(state) = app.optimizer.as_ref() {
                    app.optimizer_set_weight(state.weight - 0.1);
                }
                return false;
            }
            crossterm::event::KeyCode::Char(']') => {
                if let Some(state) = app.optimizer.as_ref() {
                    app.optimizer_set_weight(state.weight + 0.1);
                }
                return false;
            }
            crossterm::event::KeyCode::Char('0') => {
                app.optimizer_set_weight(0.0);
                return false;
            }
            crossterm::event::KeyCode::Char('1') => {
                app.optimizer_set_weight(1.0);
                return false;
            }
            _ => {}
        }
    }

    // Label edit overlay entry: `e` (lowercase, no mods) with priority graph hover > source header > panel hover.
    if matches!(key.code, crossterm::event::KeyCode::Char('e'))
        && key.modifiers.is_empty()
        && app.patch.is_some()
    {
        // 1) Graph hovered node -> Circuit
        if let Some(idx) = app.hovered_graph_node {
            if begin_graph_node_edit(app, idx) {
                return false;
            }
        }
        // 2) Source header focused -> Circuit instance at source_scroll
        let source_focused = app.showing_viewer && app.viewer_focus == ViewerFocus::Source;
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

    // Graph surface handling (`g g`). Esc closes it and restores the prior
    // view; q / Ctrl+C still quit and `l` opens the picker, mirroring the
    // viewer's global-key behavior. The graph has no focus split, so nothing
    // else is routed here.
    if app.showing_graph {
        match key.code {
            crossterm::event::KeyCode::Esc => {
                app.close_graph();
                app.prefix = None;
                return false;
            }
            crossterm::event::KeyCode::Char('q') => {
                return true;
            }
            crossterm::event::KeyCode::Char('c')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                return true;
            }
            crossterm::event::KeyCode::Char('l') => {
                app.showing_picker = true;
                app.picker_dir = std::env::current_dir().unwrap_or_default();
                app.picker_index = 0;
                app.refresh_picker_entries();
                return false;
            }
            crossterm::event::KeyCode::Char('p') => {
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
            crossterm::event::KeyCode::Char('c') => {
                // Bare `c` toggles cable latency coloring on the graph surface;
                // Ctrl+C (quit) is matched above with its modifier guard.
                app.toggle_latency_coloring();
                return false;
            }
            crossterm::event::KeyCode::Char('x') => {
                if let Some(idx) = app.hovered_graph_node {
                    graph_node_processing_toggle(app, idx);
                }
                return false;
            }
            crossterm::event::KeyCode::Char('+') | crossterm::event::KeyCode::Char('-') => {
                // Zoom family (change `tiled-window-manager`, 4.2): plain
                // scales the focused pane (graph camera zoom when the graph
                // slot is focused), `Shift` scales the other pane. The
                // camera re-seeds a fit on open; pan/zoom both re-emit the
                // image on the next draw (drag/hover/x/e are unchanged).
                let plus = matches!(key.code, crossterm::event::KeyCode::Char('+'));
                let step = if plus { 1 } else { -1 };
                if key.modifiers.contains(KeyModifiers::SHIFT) == graph_slot_focused(app) {
                    cycle_panel_scale(app, plus);
                } else {
                    app.graph_zoom_preset_step(step);
                }
                return false;
            }
            crossterm::event::KeyCode::Char('[') | crossterm::event::KeyCode::Char(']') => {
                // Zoom family (change `tiled-window-manager`, 4.2): `Alt`
                // adjusts cable tension on the focused graph pane (design D9
                // determinism holds per tension value); plain brackets adjust
                // the tiled left/right split. The optimizer menu's weight
                // slider returns before this branch.
                if key.modifiers.contains(KeyModifiers::ALT) {
                    if graph_slot_focused(app) {
                        let dir = if matches!(key.code, crossterm::event::KeyCode::Char(']')) {
                            1
                        } else {
                            -1
                        };
                        app.adjust_tension(dir);
                    }
                    return false;
                }
                let delta = if matches!(key.code, crossterm::event::KeyCode::Char('[')) {
                    -0.1
                } else {
                    0.1
                };
                app.adjust_main_split_ratio(delta);
                // Snap to a clean 0.1 step so repeated presses stay exact.
                app.main_split_ratio = (app.main_split_ratio * 10.0).round() / 10.0;
                let pct_panels = app.main_split_ratio * 100.0;
                let pct_source = 100.0 - pct_panels;
                app.status_message =
                    format!("Panels/Source split: {:.0}%/{:.0}%", pct_panels, pct_source);
                return false;
            }
            crossterm::event::KeyCode::Left
            | crossterm::event::KeyCode::Right
            | crossterm::event::KeyCode::Up
            | crossterm::event::KeyCode::Down => {
                // Task 2.3: arrows pan the graph camera on the graph surface
                // (mirrors the physical pan-on-overflow model, but gated to the
                // graph's own camera). When the camera has not been seeded yet
                // (the box-drawing path), this is a no-op so navigation is
                // unchanged.
                let (dx, dy) = match key.code {
                    crossterm::event::KeyCode::Left => (-1, 0),
                    crossterm::event::KeyCode::Right => (1, 0),
                    crossterm::event::KeyCode::Up => (0, -1),
                    crossterm::event::KeyCode::Down => (0, 1),
                    _ => (0, 0),
                };
                app.graph_pan_if_overflow(dx, dy);
                return false;
            }
            _ => {}
        }
    }

    // Embedded viewer pane handling
    if app.showing_viewer {
        // Global viewer keys: Esc, Tab, t work from either focus.
        match key.code {
            crossterm::event::KeyCode::Esc => {
                app.showing_viewer = false;
                // Defensive tile sync for pre-tiling open paths; the dispatch
                // above already removed the slot when one was open.
                app.tile_stack.close(ViewType::SourceViewer);
                app.viewer_focus = ViewerFocus::Panels;
                app.prefix = None;
                return false;
            }
            crossterm::event::KeyCode::Tab => {
                app.viewer_focus = match app.viewer_focus {
                    ViewerFocus::Source => ViewerFocus::Panels,
                    ViewerFocus::Panels => ViewerFocus::Source,
                };
                return false;
            }
            crossterm::event::KeyCode::Char('t') => {
                app.source_view_mode = match app.source_view_mode {
                    SourceViewMode::Raw => SourceViewMode::Prettified,
                    SourceViewMode::Prettified => SourceViewMode::Raw,
                };
                return false;
            }
            crossterm::event::KeyCode::Char('[') | crossterm::event::KeyCode::Char(']') => {
                // Zoom family (change `tiled-window-manager`, 4.2): brackets
                // adjust the tiled left/right split for any right-column
                // view (both ratios stay synced by the method).
                let delta = if matches!(key.code, crossterm::event::KeyCode::Char('[')) {
                    -0.1
                } else {
                    0.1
                };
                app.adjust_main_split_ratio(delta);
                // Snap to a clean 0.1 step so repeated presses stay exact
                // (avoids float drift such as 0.7000000000000001).
                app.main_split_ratio = (app.main_split_ratio * 10.0).round() / 10.0;
                let pct_panels = app.main_split_ratio * 100.0;
                let pct_source = 100.0 - pct_panels;
                app.status_message =
                    format!("Panels/Source split: {:.0}%/{:.0}%", pct_panels, pct_source);
                return false;
            }
            _ => {}
        }

        if app.viewer_focus == ViewerFocus::Source {
            // Quit still works even when source is focused.
            if matches!(key.code, crossterm::event::KeyCode::Char('q')) {
                return true;
            }
            if matches!(key.code, crossterm::event::KeyCode::Char('c'))
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                return true;
            }
            // Allow picker open even when source focused (picker precedence).
            if matches!(key.code, crossterm::event::KeyCode::Char('l')) {
                app.showing_picker = true;
                app.picker_dir = std::env::current_dir().unwrap_or_default();
                app.picker_index = 0;
                app.refresh_picker_entries();
                return false;
            }
            // Pause toggle stays live when source focused (global q/l level).
            if matches!(key.code, crossterm::event::KeyCode::Char('p')) {
                app.toggle_processing_pause();
                return false;
            }
            match key.code {
                crossterm::event::KeyCode::Char('j') => {
                    app.source_scroll = app.source_scroll.saturating_add(1);
                    return false;
                }
                crossterm::event::KeyCode::Char('k') => {
                    app.source_scroll = app.source_scroll.saturating_sub(1);
                    return false;
                }
                crossterm::event::KeyCode::Down => {
                    if app.selected_component.is_some() {
                        let next = app.occurrence_cursor.saturating_add(1);
                        app.jump_to_occurrence(next);
                    }
                    return false;
                }
                crossterm::event::KeyCode::Up => {
                    if app.selected_component.is_some() {
                        let prev = app.occurrence_cursor.saturating_sub(1);
                        app.jump_to_occurrence(prev);
                    }
                    return false;
                }
                crossterm::event::KeyCode::Home => {
                    if app.selected_component.is_some() {
                        app.jump_to_occurrence(0);
                    }
                    return false;
                }
                crossterm::event::KeyCode::End => {
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
                // Live interaction: everything else falls through to normal
                // panel handling below (shift/scale/orientation/Enter-toggle
                // work even while the source pane is focused). Only j/k and
                // Up/Down/Home/End stay routed by focus because they would
                // otherwise conflict with panel navigation.
                _ => {}
            }
        }
        // viewer_focus == Panels: fall through to normal panel handling below
        // (Esc/Tab/t already consumed).
    }

    // Label edit overlay entry (`e` on focused datum): overlay > picker > prefix > graph > source > panels.
    // Priority: graph hovered node -> viewer source header -> panel hovered token.
    if matches!(key.code, crossterm::event::KeyCode::Char('e'))
        && key.modifiers.is_empty()
        && app.editing.is_none()
    {
        // Graph surface takes precedence when open.
        if app.showing_graph {
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
        if app.showing_viewer && app.viewer_focus == ViewerFocus::Source {
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
        crossterm::event::KeyCode::Char('p') => {
            app.toggle_processing_pause();
            false
        }
        crossterm::event::KeyCode::Char('s') => {
            // Skeleton toggle (`s`): presentation switch of the main view
            // (task 3.1, design D7). Free in the normal-key path — the
            // optimizer overlay's `s` (export) returns earlier.
            app.physical_show_skeleton = !app.physical_show_skeleton;
            app.status_message = if app.physical_show_skeleton {
                String::from("Skeleton: on")
            } else {
                String::from("Skeleton: off")
            };
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
        crossterm::event::KeyCode::Char('d') if key.modifiers.is_empty() => {
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
            // When viewer is closed, Esc clears shift (and prefix already
            // handled above). When viewer was open, this branch is unreachable
            // because the viewer Esc handler returned early.
            app.active_shift = None;
            app.status_message = String::from("Shift cleared");
            app.prefix = None;
            false
        }
        crossterm::event::KeyCode::Char('o') => {
            app.orientation = match app.orientation {
                crate::app::Orientation::Portrait => crate::app::Orientation::Landscape,
                crate::app::Orientation::Landscape => crate::app::Orientation::Portrait,
            };
            app.status_message = format!(
                "Scale: {:.1} | Orientation: {:?}",
                app.scale_factor, app.orientation
            );
            false
        }
        crossterm::event::KeyCode::Char('+') | crossterm::event::KeyCode::Char('-') => {
            // Zoom family (change `tiled-window-manager`, 4.2): plain scales
            // the panels pane; `Shift` would scale the other pane, which the
            // graph arm above already owns while the graph is open.
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                return false;
            }
            cycle_panel_scale(
                app,
                matches!(key.code, crossterm::event::KeyCode::Char('+')),
            );
            false
        }
        crossterm::event::KeyCode::Char('\\') => {
            // Left-pane vertical split toggle (D3).
            app.toggle_left_split();
            app.status_message = if app.left_split_active {
                String::from("Left split on")
            } else {
                String::from("Left split off")
            };
            false
        }
        crossterm::event::KeyCode::Enter | crossterm::event::KeyCode::Char(' ') => {
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
        crossterm::event::KeyCode::Up => {
            // Physical-view pan (4.3): arrows pan the rack toward the
            // pressed direction when it overflows the main area; otherwise
            // they keep their panel-navigation meaning. j/k always navigate.
            if !app.physical_pan_if_overflow(0, -1) {
                navigate(app, -1);
            }
            false
        }
        crossterm::event::KeyCode::Down => {
            if !app.physical_pan_if_overflow(0, 1) {
                navigate(app, 1);
            }
            false
        }
        crossterm::event::KeyCode::Left => {
            app.physical_pan_if_overflow(-1, 0);
            false
        }
        crossterm::event::KeyCode::Right => {
            app.physical_pan_if_overflow(1, 0);
            false
        }
        crossterm::event::KeyCode::Char('k') => {
            navigate(app, -1);
            false
        }
        crossterm::event::KeyCode::Char('j') => {
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
    // Tiled mouse routing (change `tiled-window-manager`): the graph is a
    // right-column slot, not a full-screen surface, so its mouse handling
    // applies only while the pointer is inside the graph pane's published
    // rect. A left-click inside the tile focuses it (spec "Mouse click sets
    // focus"); events elsewhere fall through to the panel/source routing
    // below, and the pointer leaving the graph pane clears node hover so the
    // graph cannot keep a stale hover highlight.
    let graph_pane = app.pane_rects.iter().find(|(focus, rect)| {
        matches!(
            focus,
            FocusSlot::Slot(i) if app.tile_stack.slots.get(*i) == Some(&ViewType::Graph)
        ) && rect_contains(rect, mouse.column, mouse.row)
    });
    if let Some((focus, _)) = graph_pane {
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            app.tile_stack.focus = *focus;
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
                    app.select_component(token);
                }
                // Clicking a component is a panel interaction: hand keyboard
                // focus back to the panels while the viewer stays open.
                if app.showing_viewer {
                    app.viewer_focus = ViewerFocus::Panels;
                }
                app.tile_stack.focus = FocusSlot::Panels;
            } else {
                // Empty-panel-space click: clear selection without moving
                // source_scroll (deselection stability). Ignore clicks on the
                // minimap column so task 3.3 can handle click-to-scroll.
                let on_minimap = app
                    .minimap_rect
                    .is_some_and(|rect| rect_contains(&rect, mouse.column, mouse.row));
                // Bare source-pane space (no component, no minimap) focuses
                // the source pane without side effects; the selection must
                // survive so occurrence navigation keeps working there.
                let in_source_pane = app.showing_viewer
                    && !on_minimap
                    && app
                        .source_pane_rect
                        .is_some_and(|rect| rect_contains(&rect, mouse.column, mouse.row));
                if in_source_pane {
                    app.viewer_focus = ViewerFocus::Source;
                    focus_tile_slot(app, ViewType::SourceViewer);
                } else {
                    if !on_minimap {
                        app.clear_selected_component();
                    }
                    if app.showing_viewer {
                        app.viewer_focus = ViewerFocus::Panels;
                    }
                    app.tile_stack.focus = FocusSlot::Panels;
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
#[cfg(feature = "gui")]
pub fn handle_graph_window_frame(frame: &crate::gui::WindowFrame, app: &mut App) {
    let Some(mut camera) = app.graph_camera else {
        app.hovered_graph_node = None;
        return;
    };
    // Window pan/zoom reaches the shared camera (design D5): both surfaces
    // consume the same camera, so a middle-drag or wheel zoom in the window
    // moves the terminal tile identically. Applied before hit-testing so the
    // hover/drag hit rects stay aligned with the freshly painted view.
    if frame.pan_delta != (0.0, 0.0) {
        camera = crate::gui::camera_pan(&camera, frame.pan_delta.0, frame.pan_delta.1);
    }
    if let Some((factor, anchor)) = frame.zoom {
        camera = crate::gui::camera_zoom_about(&camera, factor, anchor);
    }
    app.graph_camera = Some(camera);
    // The node's pixel rect is `world × zoom − pan` at a fixed pixel size, so
    // its world size is the fixed size divided by the zoom (mirrors how the
    // terminal derives node cell rects from fixed node cell dims).
    let nw = crate::app::GRAPH_WINDOW_NODE_W / camera.zoom;
    let nh = crate::app::GRAPH_WINDOW_NODE_H / camera.zoom;
    let hit = frame.pointer.and_then(|(px, py)| {
        let (wx, wy) = camera.pixel_to_world(px, py);
        app.graph_positions
            .iter()
            .enumerate()
            .find_map(|(i, &(x, y))| {
                (wx >= x && wx < x + nw && wy >= y && wy < y + nh).then_some(i)
            })
    });
    app.hovered_graph_node = if frame.pointer.is_some() { hit } else { None };

    if frame.primary_pressed {
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
        let (wx, wy) = camera.pixel_to_world(px, py);
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
        let (wx, wy) = camera.pixel_to_world(px, py);
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
        }
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
            app.diff_picker_active = false;
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
        crossterm::event::KeyCode::Char('f') | crossterm::event::KeyCode::Char('F') => {
            // Favourites toggle for the highlighted picker entry (file-picker-favourites 3.1).
            // Directories toggle like files; the pinned section shows both. Only the
            // parent sentinel is excluded.
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
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
        crossterm::event::KeyCode::Char(d @ '0'..='9') => {
            // Digit keys fast-select a pinned favourite slot (0-based in the
            // sorted favourites list). Directories navigate in, files open like
            // Enter; out-of-range digits are silent.
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                return false;
            }
            let slot = d.to_digit(10).unwrap_or(0) as usize;
            let favs = app.picker_entries_with_favourites();
            if let Some(fav) = favs.get(slot) {
                open_picker_entry(app, fav.clone());
            }
            false
        }
        crossterm::event::KeyCode::Enter => {
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
    use crate::events::Event;
    use crate::patch::Patch;
    use ratatui::layout::Rect;
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
        // Right/Down pan positive (screen content shifts opposite, D5);
        // Left/Up reverse. Panning must not move the keyboard cursor.
        handle_event(key(crossterm::event::KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (8.0, 0.0));
        assert_eq!(app.hovered_component, None, "pan must not navigate");
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.physical_offset, (8.0, 8.0));
        handle_event(key(crossterm::event::KeyCode::Left), &mut app);
        assert_eq!(app.physical_offset, (0.0, 8.0));
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.hovered_component, Some(1));
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
        assert_eq!(app.hovered_component, Some(0));
        handle_event(key(crossterm::event::KeyCode::Left), &mut app);
        handle_event(key(crossterm::event::KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (0.0, 0.0));
    }

    #[test]
    fn arrow_pan_gated_off_in_viewer_and_graph_surfaces() {
        let mut app = app_with_overflowing_rack();
        // Viewer open (Panels focus): arrows keep panel navigation.
        app.showing_viewer = true;
        handle_event(key(crossterm::event::KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (0.0, 0.0));
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.hovered_component, Some(1));

        // Graph surface: arrows do not pan the (hidden) physical view.
        app.showing_viewer = false;
        app.showing_graph = true;
        handle_event(key(crossterm::event::KeyCode::Right), &mut app);
        assert_eq!(app.physical_offset, (0.0, 0.0));
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
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
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

    fn key(code: crossterm::event::KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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
        let quit = handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(!quit);
        assert!(app.showing_help);
    }

    #[test]
    fn question_mark_opens_help_from_every_view() {
        // Panels (default) is covered above; exercise the other surfaces.
        let mut app = App::new();
        app.showing_viewer = true;
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the viewer");

        let mut app = App::new();
        app.showing_graph = true;
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the graph");

        let mut app = App::new();
        app.showing_validation = true;
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the validation modal");

        let mut app = App::new();
        app.patch = Some(
            crate::patch::Patch::from_ini_str("[button]\n    button = B1.1\n", String::from("t"))
                .unwrap(),
        );
        assert!(app.open_optimizer());
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the optimizer");

        let mut app = App::new();
        app.showing_picker = true;
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help, "help must open over the picker");
    }

    #[test]
    fn esc_closes_help_and_returns_false() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        let quit = handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(!quit);
        assert!(!app.showing_help);
    }

    #[test]
    fn q_closes_help_without_quitting() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        // q while help is open closes help and does NOT quit.
        let quit = handle_event(key(crossterm::event::KeyCode::Char('q')), &mut app);
        assert!(!quit, "q must not quit while help is open");
        assert!(!app.showing_help);
    }

    #[test]
    fn help_eats_other_keys() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
        assert!(app.showing_help);
        // A random key while help is open is swallowed (returns false, no quit).
        let quit = handle_event(key(crossterm::event::KeyCode::Char('l')), &mut app);
        assert!(!quit);
        assert!(app.showing_help, "help stays open on unrelated keys");
    }

    #[test]
    fn click_outside_closes_help() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('?')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('-')), &mut app);
        assert_eq!(app.scale_factor, 0.75);
        assert_eq!(app.status_message, "Scaling: 75%");

        // '+' climbs back through the presets.
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 1.0);
        assert_eq!(app.status_message, "Scaling: 100%");
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 1.5);
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 2.0);

        // At the top preset, '+' wraps around to the bottom.
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_eq!(app.scale_factor, 0.75);
    }

    // Task 2.3: graph camera persistent state + wheel/arrow zoom-pan.
    //
    // A camera must be seeded (like the renderer does on the first kitty frame)
    // before zoom/pan apply; on the box-drawing path the camera is `None` so
    // both are no-ops and preserve the old navigation behavior.

    fn seed_graph_camera(app: &mut App) {
        use crate::graph_render::{GraphCamera, WorldBounds};
        app.graph_camera = Some(GraphCamera::fit_to_world(
            WorldBounds::from_positions(&app.graph_positions),
            (960.0, 480.0),
            2.2, // GRAPH_MIN_NODE_PX is kitty-gfx-gated; a literal suffices for camera math
        ));
        app.graph_canvas_px = Some((960.0, 480.0));
    }

    fn alt_key(code: crossterm::event::KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }

    fn shift_key(code: crossterm::event::KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
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
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        let z1 = app.graph_camera.unwrap().zoom;
        assert!(
            (z1 - z0 * 1.5).abs() < 1e-2,
            "'+' zooms in one preset: {z0} -> {z1}"
        );
        // Presets are now multipliers of the fitted zoom: 1.0 is index 5,
        // one '+' lands on 1.5 (index 6).
        assert_eq!(app.graph_zoom_preset, 6);
        assert!(app.status_message.contains("Graph zoom"));

        // Wrap at the top preset (200%) back to the bottom (6.25%): the
        // deep zoom-out steps exist so a fitted camera can reach the true
        // fit of a large patch (bug droid_tui-ttz).
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        let z2 = app.graph_camera.unwrap().zoom;
        assert!((z2 - z1 * (2.0 / 1.5)).abs() < 1e-2);
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        let z3 = app.graph_camera.unwrap().zoom;
        assert!(
            (z3 - z2 * (0.0625 / 2.0)).abs() < 1e-2,
            "wrap from 200% to 6.25%: {z2} -> {z3}"
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
        handle_event(key(crossterm::event::KeyCode::Right), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
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
        // Default preset is the fitted zoom (1.0, index 5), not a stale one.
        assert_eq!(app.graph_zoom_preset, 5);
        assert!(app.graph_canvas_px.is_none());
    }

    #[test]
    fn picker_enter_on_parent_entry_navigates_up_without_closing() {
        let mut app = picker_app_at("fixtures/picker_test");
        assert!(
            is_picker_parent_entry(&app.picker_entries[0]),
            "parent entry is the '..' sentinel"
        );
        app.picker_index = 0;
        handle_picker_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert!(app.showing_picker, "picker stays open when navigating up");
        assert_eq!(app.picker_dir, std::path::PathBuf::from("fixtures"));
        assert!(app.patch.is_none());
    }

    /// Open the embedded source viewer via `g` then `v`.
    fn open_viewer(app: &mut App) {
        handle_event(key(crossterm::event::KeyCode::Char('g')), app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), app);
        assert!(app.showing_viewer);
    }

    #[test]
    fn bracket_split_keys_noop_when_viewer_closed() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.6);
        handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.6);
        assert_eq!(
            app.status_message,
            String::from("No patch loaded. Press 'l' to load.")
        );
    }

    #[test]
    fn close_bracket_increases_split_ratio_by_0_1_and_clamps_at_0_7() {
        let mut app = App::new();
        open_viewer(&mut app);
        handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.7);
        assert_eq!(app.status_message, "Panels/Source split: 70%/30%");
        // Further presses clamp at the upper bound.
        handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.7);
    }

    #[test]
    fn open_bracket_decreases_split_ratio_by_0_1_and_clamps_at_0_3() {
        let mut app = App::new();
        open_viewer(&mut app);
        // Steps 0.6 -> 0.5 -> 0.4 -> 0.3.
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.5);
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.4);
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.3);
        assert_eq!(app.status_message, "Panels/Source split: 30%/70%");
        // Further presses clamp at the lower bound.
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.3);
    }

    /// Open the optimizer menu via `g` then `o`.
    fn open_optimizer(app: &mut App) {
        handle_event(key(crossterm::event::KeyCode::Char('g')), app);
        handle_event(key(crossterm::event::KeyCode::Char('o')), app);
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
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('o')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().cursor, 1);
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
        let previewed = app.optimizer.as_ref().unwrap().previewing;
        assert_eq!(previewed, Some(0));
        assert!(app.graph.is_some());
        assert!(
            app.status_message.contains("Preview:"),
            "unexpected status: {:?}",
            app.status_message
        );

        // r restores the original order.
        handle_event(key(crossterm::event::KeyCode::Char('r')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert!(app.optimizer.as_ref().unwrap().previewing.is_some());
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.0);

        // `0`/`1` snap straight to the endpoints.
        handle_event(key(crossterm::event::KeyCode::Char('1')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 1.0);
        handle_event(key(crossterm::event::KeyCode::Char('0')), &mut app);
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.0);

        // `]` past the top clamps at 1.0; `[` past the bottom clamps at 0.0.
        handle_event(key(crossterm::event::KeyCode::Char('1')), &mut app);
        for _ in 0..5 {
            handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
        }
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 1.0);
        // Ten steps down from 1.0 reach 0.0; an eleventh `[` must stay at the floor.
        for _ in 0..11 {
            handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        }
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.0);

        // Esc still closes the menu; `g o` still reopens it.
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(app.optimizer.is_none());
        open_optimizer(&mut app);
        assert!(app.optimizer.is_some());
    }

    #[test]
    fn optimizer_weight_keys_do_not_shift_viewer_split() {
        // The optimizer overlay owns `[`/`]` while open (it returns before the
        // viewer-split branch); closing the menu must hand them back.
        let mut app = app_with_fixture();
        open_optimizer(&mut app);
        handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
        assert_eq!(
            app.viewer_split_ratio, 0.6,
            "optimizer `]` must not adjust the viewer split"
        );
        assert_eq!(app.optimizer.as_ref().unwrap().weight, 0.1);
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        // With the menu closed `[`/`]` go back to the viewer split (no-op
        // without the viewer open).
        handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
        assert_eq!(app.viewer_split_ratio, 0.6);
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
        handle_event(key(crossterm::event::KeyCode::Char('s')), &mut app);
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
    fn picker_enter_keys_label_store_path() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.picker_index = picker_index_of(&app, "patch_a.ini");
        handle_picker_event(key(crossterm::event::KeyCode::Enter), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Enter), &mut app);
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
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert!(app.prefix.is_none());
    }

    #[test]
    fn g_then_v_initial_position_bof_when_no_selection() {
        let mut app = app_with_source_navigation();
        // No selection -> BOF
        app.source_scroll = 99;
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, first);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.selected_component, Some(String::from("B1.1")));
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

    // `g w` / `g g` GPU-graph-window keys (gpu-graph-window 3.2). The handler
    // only records requests; the windowed loop in main.rs consumes them.

    #[cfg(not(feature = "gui"))]
    #[test]
    fn g_w_without_gui_feature_shows_status_hint() {
        let mut app = app_with_fixture();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        let quit = handle_event(key(crossterm::event::KeyCode::Char('w')), &mut app);
        assert!(!quit);
        assert!(app.prefix.is_none());
        assert_eq!(
            app.status_message,
            "GPU graph window requires the `gui` feature"
        );
    }

    #[cfg(feature = "gui")]
    #[test]
    fn g_w_queues_window_toggle_request() {
        let mut app = app_with_fixture();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        let quit = handle_event(key(crossterm::event::KeyCode::Char('w')), &mut app);
        assert!(!quit);
        assert!(app.prefix.is_none());
        assert_eq!(app.take_graph_window_request(), GraphWindowRequest::Toggle);
        // One-shot: the consumer's poll-and-clear leaves nothing behind.
        assert_eq!(app.take_graph_window_request(), GraphWindowRequest::None);
    }

    #[cfg(feature = "gui")]
    #[test]
    fn g_g_with_window_enabled_queues_open_without_tile() {
        let mut app = app_with_fixture();
        app.graph_window_enabled = true;
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        let quit = handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(!quit);
        assert!(!app.showing_graph, "window replaces the terminal tile");
        assert!(app.tile_stack.slots.is_empty());
        assert_eq!(app.take_graph_window_request(), GraphWindowRequest::Open);
    }

    #[cfg(feature = "gui")]
    #[test]
    fn g_g_with_window_disabled_still_opens_terminal_tile() {
        let mut app = app_with_fixture();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        let quit = handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(!quit);
        assert!(app.showing_graph, "default: `g g` opens the terminal tile");
        assert_eq!(app.take_graph_window_request(), GraphWindowRequest::None);
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
    fn g_after_timeout_rearms_prefix() {
        // Non-interference (task 4.3): a `g` whose prefix already timed out
        // clears the stale prefix and re-arms a fresh one rather than acting
        // as a follow-up key.
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        app.prefix = Some(PrefixState {
            started: Instant::now() - Duration::from_secs(2),
        });
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some(), "first g arms the prefix");
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);
        let default = crate::layout::DEFAULT_TENSION;
        assert_eq!(app.tension, default);
        let before = app.graph_positions.clone();

        handle_event(alt_key(crossterm::event::KeyCode::Char(']')), &mut app);
        assert_eq!(app.tension, default + crate::layout::TENSION_STEP);
        assert_eq!(
            app.status_message,
            format!("Cable tension: {:.2}", app.tension)
        );
        assert_ne!(app.graph_positions, before, "tension change re-solves");

        handle_event(alt_key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.tension, default);
        assert_eq!(
            app.graph_positions, before,
            "same tension reproduces layout"
        );
    }

    #[test]
    fn zoom_family_plain_and_shift_route_by_focus() {
        // Graph slot focused: plain `+` zooms the camera, `Shift+`+`` scales
        // the panels pane instead.
        let mut app = app_with_fixture();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        seed_graph_camera(&mut app);
        let z0 = app.graph_camera.unwrap().zoom;
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert!((app.graph_camera.unwrap().zoom - z0 * 1.5).abs() < 1e-2);
        let scale_before = app.scale_factor;
        handle_event(shift_key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_ne!(app.scale_factor, scale_before, "other pane scales");
        assert!((app.graph_camera.unwrap().zoom - z0 * 1.5).abs() < 1e-2);
        // Panels focused: plain `+` scales panels, camera untouched.
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Panels);
        let z1 = app.graph_camera.unwrap().zoom;
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert!((app.graph_camera.unwrap().zoom - z1).abs() < 1e-9);
    }

    #[test]
    fn brackets_adjust_main_split_ratio() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        handle_event(key(crossterm::event::KeyCode::Char(']')), &mut app);
        assert_eq!(app.main_split_ratio, 0.7);
        assert_eq!(app.viewer_split_ratio, 0.7, "ratios stay synced");
        assert_eq!(app.status_message, "Panels/Source split: 70%/30%");
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('[')), &mut app);
        assert_eq!(app.main_split_ratio, 0.5);
    }

    #[test]
    fn backslash_toggles_left_split() {
        let mut app = App::new();
        assert!(!app.left_split_active);
        handle_event(key(crossterm::event::KeyCode::Char('\\')), &mut app);
        assert!(app.left_split_active);
        assert_eq!(app.status_message, "Left split on");
        handle_event(key(crossterm::event::KeyCode::Char('\\')), &mut app);
        assert!(!app.left_split_active);
        assert_eq!(app.status_message, "Left split off");
    }

    #[test]
    fn viewer_esc_closes_keeping_selection_and_scroll() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        let scroll = app.source_scroll;
        let sel = app.selected_component.clone();
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(!app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        assert_eq!(app.selected_component, sel, "selection kept on close");
        assert_eq!(app.source_scroll, scroll, "scroll kept on close");
        assert!(app.prefix.is_none());
    }

    #[test]
    fn viewer_j_k_scroll_when_source_focused() {
        let mut app = app_with_source_navigation();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, 0);
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert_eq!(app.source_scroll, 1);
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert_eq!(app.source_scroll, 3);
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 2);
        // Saturate at 0
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 0);
    }

    #[test]
    fn t_toggles_view_mode_when_viewer_open() {
        let mut app = app_with_source_navigation();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
        handle_event(key(crossterm::event::KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Prettified);
        handle_event(key(crossterm::event::KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    }

    #[test]
    fn t_noop_when_viewer_closed() {
        let mut app = App::new();
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
        handle_event(key(crossterm::event::KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    }

    #[test]
    fn tab_switches_focus_when_viewer_open() {
        let mut app = app_with_source_navigation();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
    }

    #[test]
    fn tab_noop_when_viewer_closed() {
        let mut app = App::new();
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        assert!(!app.showing_viewer);
    }

    // ── Task 4.1: focused-pane dispatch (`tiled-window-manager`, D7) ──

    fn shift_tab() -> KeyEvent {
        KeyEvent::new(crossterm::event::KeyCode::BackTab, KeyModifiers::SHIFT)
    }

    #[test]
    fn tiled_tab_cycles_focus_across_panes() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Slot(0));
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.showing_graph);
        assert_eq!(app.tile_stack.focus, FocusSlot::Slot(1));
        // Forward: graph slot -> panels -> viewer slot -> graph slot.
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Panels);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Slot(0));
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Slot(1));
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        // Backward from the graph slot lands on the viewer slot.
        handle_event(shift_tab(), &mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Slot(0));
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
    }

    #[test]
    fn tiled_esc_closes_focused_view_keeps_others() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert_eq!(app.tile_stack.slots.len(), 2);
        // Graph slot focused: Esc closes it, viewer survives.
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(!app.showing_graph);
        assert!(app.showing_viewer);
        assert_eq!(app.tile_stack.focus, FocusSlot::Panels);
    }

    #[test]
    fn tiled_esc_on_panels_clears_selection_keeps_views() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        open_viewer(&mut app);
        // Back to panels: Esc clears the selection, viewer stays open.
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Panels);
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(app.showing_viewer);
        assert!(app.selected_component.is_none());
    }

    #[test]
    fn viewer_focus_source_live_panel_keys() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);

        // Shift keys work live while source is focused.
        handle_event(key(crossterm::event::KeyCode::Char('1')), &mut app);
        assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
        assert_eq!(app.status_message, "Shift 1 active");

        // Scale preset cycling works live.
        let scale_before = app.scale_factor;
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_ne!(
            app.scale_factor, scale_before,
            "scale live when source focused"
        );

        // Orientation toggle works live.
        let orient_before = app.orientation.clone();
        handle_event(key(crossterm::event::KeyCode::Char('o')), &mut app);
        assert_ne!(
            app.orientation, orient_before,
            "orientation live when source focused"
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
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert_ne!(
            app.patch.as_ref().unwrap().hw_components[b11_idx].state,
            state_before,
            "Enter toggles while source focused"
        );
        assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
        assert_eq!(app.source_scroll, first_b11);
        assert_eq!(app.occurrence_cursor, 0);
        // Space toggles back, still live.
        handle_event(key(crossterm::event::KeyCode::Char(' ')), &mut app);
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[b11_idx].state,
            state_before,
            "Space toggles while source focused"
        );

        // j/k and Up/Down/Home/End remain routed by focus (they would
        // otherwise conflict with panel navigation).
        let scroll = app.source_scroll;
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert_eq!(
            app.source_scroll,
            scroll + 1,
            "j scrolls source when focused"
        );
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, 1, "Down navigates occurrences");
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
        assert_eq!(app.occurrence_cursor, 0, "Up navigates occurrences");
    }

    #[test]
    fn viewer_focus_panels_allows_panel_keys() {
        let mut app = app_with_source_navigation();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        // Switch to panels
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(crossterm::event::KeyCode::Char('1')), &mut app);
        assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
        let scale_before = app.scale_factor;
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_ne!(app.scale_factor, scale_before);
        let orient_before = app.orientation.clone();
        handle_event(key(crossterm::event::KeyCode::Char('o')), &mut app);
        assert_ne!(app.orientation, orient_before);
        app.hovered_component = Some(0);
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // Down -> 1
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, 1);
        assert_eq!(app.source_scroll, occurrences[1].line);
        // Down -> 2
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, 2);
        // saturate at bounds: press Down many times, should end at last
        for _ in 0..10 {
            handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        }
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        // Up -> back one
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 2);
        // Home -> 0
        handle_event(key(crossterm::event::KeyCode::Home), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // End -> last
        handle_event(key(crossterm::event::KeyCode::End), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        assert_eq!(app.source_scroll, occurrences.last().unwrap().line);
    }

    #[test]
    fn mouse_click_component_works_when_source_focused() {
        let mut app = app_with_source_navigation();
        open_viewer(&mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
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
    }

    #[test]
    fn mouse_click_source_pane_space_focuses_source_without_side_effects() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        open_viewer(&mut app);
        // Start from panels focus to prove a bare source-pane click switches it.
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        app.source_pane_rect = Some(Rect::new(60, 3, 40, 20));
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
        assert_eq!(
            app.tile_stack.focus,
            FocusSlot::Slot(0),
            "viewer open takes the source slot focus"
        );
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 1),
            &mut app,
        );
        assert_eq!(
            app.tile_stack.focus,
            FocusSlot::Panels,
            "component click hands focus to the panels slot"
        );
    }

    #[test]
    fn tiled_mouse_click_source_pane_focuses_source_slot() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        open_viewer(&mut app);
        // Start from panels focus to prove a bare source-pane click switches
        // the tiled focus back to the source slot.
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.tile_stack.focus, FocusSlot::Panels);
        app.source_pane_rect = Some(Rect::new(60, 3, 40, 20));
        app.minimap_rect = None;
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 80, 10),
            &mut app,
        );
        assert_eq!(
            app.tile_stack.focus,
            FocusSlot::Slot(0),
            "source-pane click focuses the source slot"
        );
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
    }

    #[test]
    fn tiled_mouse_click_graph_pane_focuses_graph_slot() {
        // Spec "Mouse click sets focus": a click inside the graph pane moves
        // tiled focus to the graph slot. Also pins tiled mouse routing: the
        // graph tile must not swallow clicks aimed at the panels pane.
        let mut app = app_with_source_navigation();
        // Slot 0 is the source viewer, slot 1 the graph.
        app.open_view(ViewType::SourceViewer);
        app.open_graph();
        app.tile_stack.focus = FocusSlot::Slot(0);
        // Renderer-published rects: panels left, two stacked slots right.
        app.pane_rects = vec![
            (FocusSlot::Panels, Rect::new(0, 0, 40, 40)),
            (FocusSlot::Slot(0), Rect::new(40, 0, 120, 20)),
            (FocusSlot::Slot(1), Rect::new(40, 20, 120, 20)),
        ];
        // Click inside the graph pane (no node): focus moves to the graph
        // slot without starting a drag.
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 100, 25),
            &mut app,
        );
        assert_eq!(
            app.tile_stack.focus,
            FocusSlot::Slot(1),
            "click inside the graph pane focuses the graph slot"
        );
        assert!(app.graph_drag.is_none(), "no node under the click");

        // A click on a panel component must reach the panels pane even while
        // the graph tile is open.
        app.component_rects = vec![(0, Rect::new(2, 2, 16, 2))];
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 3),
            &mut app,
        );
        assert_ne!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before,
            "panel component click toggles while the graph tile is open"
        );
        assert_eq!(
            app.tile_stack.focus,
            FocusSlot::Panels,
            "component click hands focus to the panels slot"
        );
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
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char(' ')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char(' ')), &mut app);
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
        let scroll_before = app.source_scroll;
        let idx0 = idx_for(&app, "B1.1");
        app.component_rects = vec![(idx0, Rect::new(0, 0, 16, 2))];
        app.minimap_rect = Some(Rect::new(70, 0, 10, 20));
        // Click inside minimap (empty relative to components but on minimap)
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 75, 5),
            &mut app,
        );
        // Selection preserved, scroll preserved (minimap click handled in 3.3)
        assert_eq!(app.selected_component, Some(String::from("B1.1")));
        assert_eq!(app.source_scroll, scroll_before);
    }

    #[test]
    fn occurrence_navigation_no_selection_noop() {
        let mut app = app_with_source_navigation();
        // Ensure no selection, viewer open and source focused
        assert!(app.selected_component.is_none());
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        app.source_scroll = 5;
        app.occurrence_cursor = 0;
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
        assert_eq!(app.source_scroll, 5);
        assert_eq!(app.occurrence_cursor, 0);
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.source_scroll, 5);
        handle_event(key(crossterm::event::KeyCode::Home), &mut app);
        assert_eq!(app.source_scroll, 5);
        handle_event(key(crossterm::event::KeyCode::End), &mut app);
        assert_eq!(app.source_scroll, 5);
    }

    #[test]
    fn occurrence_navigation_saturates_at_bounds_via_handler() {
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        let occurrences = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
        assert!(occurrences.len() >= 2);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        // Already at first occurrence after select
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // Up at first saturates
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
        // Down to last saturates
        for _ in 0..occurrences.len() + 5 {
            handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        }
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        assert_eq!(app.source_scroll, occurrences.last().unwrap().line);
        // Down while at last stays
        handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        // Home -> first, End -> last
        handle_event(key(crossterm::event::KeyCode::Home), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        handle_event(key(crossterm::event::KeyCode::End), &mut app);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
    }

    #[test]
    fn j_k_scroll_remains_when_viewer_open() {
        let mut app = app_with_source_navigation();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, 0);
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert_eq!(app.source_scroll, 1);
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 0);
        // j/k saturate at 0
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        assert_eq!(app.source_scroll, 0);
    }

    #[test]
    fn esc_clears_prefix_when_viewer_closed() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(app.prefix.is_none());
        // When viewer open and source focused, g is live too and arms the prefix.
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some(), "g arms even when source focused");
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        // Esc first clears the prefix, viewer stays open
        assert!(app.showing_viewer);
        assert!(app.prefix.is_none());
        // After Tab to panels, g arms again.
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        // Esc first clears the prefix, viewer stays open
        assert!(app.showing_viewer);
        assert!(app.prefix.is_none());
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        // Panels focused: Esc clears the selection, the viewer slot stays.
        assert!(app.showing_viewer);
        assert!(app.selected_component.is_none());
        // Tab back to the viewer slot, Esc closes the focused view.
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
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
        // Tiled renderer contract (change `tiled-window-manager`): the graph
        // is a right-column slot whose published pane rect gates mouse
        // routing. Mirror a rendered layout so graph events route to graph
        // handling instead of falling through to the panels pane.
        app.pane_rects = vec![
            (FocusSlot::Panels, Rect::new(0, 0, 8, 40)),
            (FocusSlot::Slot(0), Rect::new(8, 0, 192, 40)),
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
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
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
        let quit = handle_event(key(crossterm::event::KeyCode::Char('q')), &mut app);
        assert!(quit, "q quits even with the graph open");
    }

    #[test]
    fn l_opens_picker_while_graph_open() {
        let mut app = app_with_fixture();
        app.open_graph();
        handle_event(key(crossterm::event::KeyCode::Char('l')), &mut app);
        assert!(app.showing_picker, "l opens the picker over the graph");
    }

    #[test]
    fn drag_node_moves_position_resettles_and_emits_node_moved() {
        let mut app = graph_app();
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

    #[cfg(feature = "gui")]
    #[test]
    fn graph_window_frame_press_selects_circuit() {
        use crate::gui::WindowFrame;
        let mut app = app_with_graph_window();
        let node_id = app.graph.as_ref().unwrap().nodes[0].id.clone();
        let (x, y) = app.graph_positions[0];
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0, y + 5.0)),
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

    // ── 5.1 regression anchoring inside handler.rs (fixtures/source_navigation.ini) ──
    // Each test below drives real flows end-to-end through handle_event/handle_mouse_event + render
    // so they break if geometry, prefix, or viewer routing drifts. The dedicated
    // src/regression.rs holds the full suite; these smoke tests anchor the
    // same coverage directly in handler.rs per task 5.1 scope requirement.
    #[test]
    fn regression_handler_e2e_initial_bof_and_selected_open() {
        let mut app = app_with_source_navigation();
        app.source_scroll = 77;
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        assert_eq!(app.source_scroll, 0, "BOF when no selection");
        // selected-open jumps to first occurrence
        let mut app2 = app_with_source_navigation();
        let first = app2.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
        app2.select_component(String::from("B1.1"));
        app2.source_scroll = 999;
        app2.showing_viewer = false;
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app2);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app2);
        assert_eq!(app2.source_scroll, first);
        assert_eq!(app2.occurrence_cursor, 0);
    }

    #[test]
    fn regression_handler_e2e_t_and_tab_and_picker_and_isolation() {
        let mut app = app_with_source_navigation();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        // t preserves usable content: toggles but stays in bounds
        let scroll_before = app.source_scroll;
        handle_event(key(crossterm::event::KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, crate::app::SourceViewMode::Prettified);
        assert_eq!(app.source_scroll, scroll_before);
        handle_event(key(crossterm::event::KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, crate::app::SourceViewMode::Raw);
        // Tab round-trip Source->Panels->Source
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        // picker precedence: l opens picker even when source focused
        handle_event(key(crossterm::event::KeyCode::Char('l')), &mut app);
        assert!(app.showing_picker, "picker overlays viewer");
        // while picker open, t is inert
        let mode_before = app.source_view_mode.clone();
        handle_event(key(crossterm::event::KeyCode::Char('t')), &mut app);
        assert_eq!(app.source_view_mode, mode_before);
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(!app.showing_picker);
        assert!(app.showing_viewer);
        // live interaction: panel keys work even when Source focused
        if app.viewer_focus != ViewerFocus::Source {
            handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        }
        let scale_before = app.scale_factor;
        handle_event(key(crossterm::event::KeyCode::Char('+')), &mut app);
        assert_ne!(
            app.scale_factor, scale_before,
            "scale live when Source focused"
        );
        handle_event(key(crossterm::event::KeyCode::Char('1')), &mut app);
        assert_eq!(
            app.active_shift,
            Some(ShiftGroup::Group1),
            "shift live when Source focused"
        );
    }

    #[test]
    fn regression_handler_e2e_minimap_and_deselect_and_bounds() {
        use crate::ui::render;
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = app_with_source_navigation();
        app.select_component(String::from("B1.1"));
        let occ = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        // occurrence bounds: Up saturates at 0, End/Down saturate at last
        handle_event(key(crossterm::event::KeyCode::Up), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        for _ in 0..occ.len() + 3 {
            handle_event(key(crossterm::event::KeyCode::Down), &mut app);
        }
        assert_eq!(app.occurrence_cursor, occ.len() - 1);
        handle_event(key(crossterm::event::KeyCode::Home), &mut app);
        assert_eq!(app.occurrence_cursor, 0);
        handle_event(key(crossterm::event::KeyCode::End), &mut app);
        assert_eq!(app.occurrence_cursor, occ.len() - 1);
        // deselect keeps position
        handle_event(key(crossterm::event::KeyCode::Home), &mut app);
        let pos = app.source_scroll;
        handle_event(key(crossterm::event::KeyCode::Tab), &mut app);
        let idx = app
            .patch
            .as_ref()
            .unwrap()
            .hw_components
            .iter()
            .position(|c| c.id == "B1.1")
            .unwrap();
        app.component_rects = vec![(idx, Rect::new(0, 0, 16, 2))];
        app.minimap_rect = None;
        {
            let backend = TestBackend::new(120, 40);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
        }
        app.minimap_rect = None;
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 100, 50),
            &mut app,
        );
        assert!(app.selected_component.is_none());
        assert_eq!(app.source_scroll, pos, "deselect must not move scroll");
        // minimap click maps correctly
        let mut app2 = app_with_source_navigation();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app2);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app2);
        {
            let backend = TestBackend::new(120, 40);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render(frame, &mut app2)).unwrap();
        }
        let rect = app2.minimap_rect.expect("minimap visible");
        let x = rect.x + 1;
        let top_y = rect.y + 1;
        let bot_y = rect.y + rect.height.saturating_sub(2);
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), x, top_y),
            &mut app2,
        );
        let top = app2.source_scroll;
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), x, bot_y),
            &mut app2,
        );
        let bot = app2.source_scroll;
        assert!(top <= bot, "minimap top <= bottom");
        assert!(top <= 5, "top near BOF");
    }

    // ── Task 2.1: global processing pause (`p`) ──

    #[test]
    fn p_toggles_processing_pause_with_status() {
        let mut app = app_with_fixture();
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        assert!(app.processing_paused);
        assert_eq!(app.status_message, "Processing paused (p to resume)");
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        assert!(!app.pinned.contains(&node.id), "second p unpins");
        assert_eq!(
            app.status_message,
            format!("Unpinned: {} {}", node.circuit, node.instance_index)
        );
    }

    #[test]
    fn p_noop_while_picker_open() {
        let mut app = picker_app_at("fixtures/picker_test");
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        assert!(!app.processing_paused, "picker swallows p");
    }

    #[test]
    fn enter_and_space_do_not_mutate_while_paused() {
        let mut app = app_with_fixture();
        app.hovered_component = Some(0);
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        let state_before = app.patch.as_ref().unwrap().hw_components[0].state.clone();
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            state_before,
            "Enter must not toggle while paused"
        );
        handle_event(key(crossterm::event::KeyCode::Char(' ')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
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
    fn c_key_toggles_latency_coloring_on_graph_surface() {
        // Bare `c` flips the graph-surface cable coloring; it must never
        // collide with Ctrl+C (quit), which carries a modifier.
        let mut app = app_with_fixture();
        assert!(app.latency_coloring, "on by default");
        app.open_graph();

        handle_event(key(crossterm::event::KeyCode::Char('c')), &mut app);
        assert!(!app.latency_coloring);
        assert_eq!(app.status_message, "Latency coloring off (c to toggle)");

        handle_event(key(crossterm::event::KeyCode::Char('c')), &mut app);
        assert!(app.latency_coloring);
        assert_eq!(app.status_message, "Latency coloring on (c to toggle)");
    }

    #[test]
    fn scroll_adjustment_blocked_while_paused() {
        let content = "[pot]\n    pot = P1.1\n    output = _X\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
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
        app
    }

    #[test]
    fn graph_x_disables_hovered_node_and_rebuilds() {
        let mut app = app_with_graph();
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        app.hovered_graph_node = Some(0);
        let before_positions = app.graph_positions.clone();
        handle_event(key(crossterm::event::KeyCode::Char('x')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('x')), &mut app);
        assert!(app
            .disabled_circuits
            .contains(&NodeId::circuit(&node.circuit, node.instance_index)));
        // Second x on same hovered node re-enables.
        handle_event(key(crossterm::event::KeyCode::Char('x')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('x')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
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
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        assert!(!app.pinned.contains(&node.id), "second p unpins");
        assert!(!app.processing_paused, "pause untouched throughout");
    }

    // --- GPU graph window interaction mapping (task 3.1) --------------------
    // The window hit-tests pointers against the graph layout via the camera;
    // with an identity camera (zoom 1, pan 0) world == pixel, so pointers can
    // be taken straight from `graph_positions`.

    #[cfg(feature = "gui")]
    fn app_with_graph_window() -> App {
        let mut app = app_with_graph();
        app.graph_camera = Some(crate::graph_render::GraphCamera::new());
        app
    }

    /// Pointer just inside `target`'s top-left corner, accepted only when no
    /// earlier node's rect also claims it (hit-testing takes the first match),
    /// so the sample deterministically hits `target`.
    #[cfg(feature = "gui")]
    fn clean_sample_point(app: &App, target: usize) -> Option<(f32, f32)> {
        let (px, py) = app.graph_positions[target];
        let (sx, sy) = (px + 5.0, py + 5.0);
        let nw = crate::app::GRAPH_WINDOW_NODE_W;
        let nh = crate::app::GRAPH_WINDOW_NODE_H;
        let claims = |i: usize| {
            let (nx, ny) = app.graph_positions[i];
            sx >= nx && sx < nx + nw && sy >= ny && sy < ny + nh
        };
        (claims(target) && !(0..target).any(claims)).then_some((sx, sy))
    }

    #[cfg(feature = "gui")]
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
        // A pointer beyond every node's right edge misses all rects.
        let max_right = app
            .graph_positions
            .iter()
            .map(|&(nx, _)| nx + crate::app::GRAPH_WINDOW_NODE_W)
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

    #[cfg(feature = "gui")]
    #[test]
    fn graph_window_drag_moves_node_and_auto_pins_on_release() {
        use crate::gui::WindowFrame;
        let mut app = app_with_graph_window();
        let (x, y) = app.graph_positions[0];
        let node_id = app.graph.as_ref().unwrap().nodes[0].id.clone();
        // Press on node 0: grabs the node, no move yet (mirrors the terminal
        // Down/Drag split).
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0, y + 5.0)),
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
                pointer: Some((x + 35.0, y + 25.0)),
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

    #[cfg(feature = "gui")]
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
        let (x, y) = app.graph_positions[0];
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0, y + 5.0)),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 35.0, y + 25.0)),
                primary_down: true,
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(received.borrow().len(), 1, "one NodeMoved per drag frame");
        assert!(matches!(received.borrow()[0], Event::NodeMoved(_)));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn graph_window_keys_act_on_hovered_node() {
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        let (x, y) = app.graph_positions[0];
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0, y + 5.0)),
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

    #[cfg(feature = "gui")]
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

    #[cfg(feature = "gui")]
    #[test]
    fn graph_window_edit_key_opens_circuit_overlay() {
        use crate::app::EditKind;
        use crate::gui::{WindowFrame, WindowGraphKey};
        let mut app = app_with_graph_window();
        let (x, y) = app.graph_positions[0];
        let node = app.graph.as_ref().unwrap().nodes[0].clone();
        handle_graph_window_frame(
            &WindowFrame {
                pointer: Some((x + 5.0, y + 5.0)),
                keys: vec![WindowGraphKey::BeginEdit],
                ..Default::default()
            },
            &mut app,
        );
        let editing = app.editing.as_ref().expect("e opens the edit overlay");
        assert_eq!(editing.kind, EditKind::Circuit { node: node.id });
    }

    #[cfg(feature = "gui")]
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
        handle_graph_window_frame(
            &WindowFrame {
                zoom: Some((1.5, anchor)),
                ..Default::default()
            },
            &mut app,
        );
        assert_eq!(
            app.graph_camera.unwrap(),
            camera_zoom_about(&panned, 1.5, anchor),
            "window wheel zoom reaches the shared camera"
        );
    }

    #[cfg(feature = "gui")]
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('f')), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('f')), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('F')), &mut app);
        assert!(
            app.favorites.is_favourite(&dummy),
            "F should also mark favourite"
        );
        handle_picker_event(key(crossterm::event::KeyCode::Char('F')), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('f')), &mut app);
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
        let ctrl_f = KeyEvent::new(crossterm::event::KeyCode::Char('f'), KeyModifiers::CONTROL);
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
        let quit = handle_picker_event(key(crossterm::event::KeyCode::Enter), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('f')), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('f')), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('1')), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('0')), &mut app);
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
        handle_picker_event(key(crossterm::event::KeyCode::Char('9')), &mut app);
        assert!(app.showing_picker);
        assert!(app.patch.is_none());
        assert!(app.selected_file.is_none());

        // Ctrl/Alt-held digits never fire (mirrors the f-handler guard).
        let ctrl_zero = KeyEvent::new(crossterm::event::KeyCode::Char('0'), KeyModifiers::CONTROL);
        handle_picker_event(ctrl_zero, &mut app);
        assert!(app.showing_picker, "Ctrl+digit must not open a favourite");
        assert!(app.patch.is_none());
        let alt_zero = KeyEvent::new(crossterm::event::KeyCode::Char('0'), KeyModifiers::ALT);
        handle_picker_event(alt_zero, &mut app);
        assert!(app.showing_picker, "Alt+digit must not open a favourite");
        assert!(app.patch.is_none());

        match orig_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn render_outlier_hint_never_blocks_input_or_loading() {
        use crate::ui::render;
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        // Degraded render (arpeggio1 wants 228 cols; frame is 80) must not
        // block load_patch, must show the advisory hint in the status bar,
        // and must never intercept keyboard input.
        let mut app = App::new();
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        assert!(
            app.load_patch(patch),
            "degraded render must not block load_patch"
        );

        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            text.contains("Renders degraded at 80 cols"),
            "hint must render while the degraded patch is loaded"
        );

        // Keyboard input keeps flowing while the hint is visible.
        handle_event(key(crossterm::event::KeyCode::Char('-')), &mut app);
        assert_eq!(app.scale_factor, 0.75, "keyboard input not intercepted");
    }
}
