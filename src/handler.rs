use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use std::collections::HashMap;

use crate::app::{
    is_entry_selectable, App, GraphDrag, PrefixState, QuadFocus, SourceViewMode, ViewerFocus,
};
use crate::layout;
use crate::patch::{ComponentKind, ComponentState, HwComponent, Patch, ShiftGroup};

/// How long an armed `g` prefix waits for its follow-up key before silently
/// cancelling. The timeout is lazy: it is checked only when the next event
/// arrives, so no timer thread or event-loop change is needed.
const PREFIX_TIMEOUT: Duration = Duration::from_secs(1);

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
            || (app.showing_viewer && app.viewer_focus == ViewerFocus::Source)
            || (app.showing_quad && app.quad_focus == QuadFocus::Source);
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
                if app.showing_quad {
                    app.close_quad();
                }
                open_embedded_viewer(app);
                return false;
            }
            crossterm::event::KeyCode::Char('g') => {
                // `g g` opens the graph surface, mirroring `g v` (design D7).
                if app.showing_quad {
                    app.close_quad();
                }
                app.open_graph();
                app.prefix = None;
                return false;
            }
            crossterm::event::KeyCode::Char('q') => {
                // `g q` opens the quad concurrent view (panels | source / FULL | FILTERED).
                app.open_quad();
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
            crossterm::event::KeyCode::Esc => {
                app.prefix = None;
                return false;
            }
            _ => {
                app.prefix = None;
            }
        }
    }

    // Diff overlay Esc handling: clear scope first, then overlay (before viewer/graph/quad Esc).
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

    // Label edit overlay entry: `e` (lowercase, no mods) with priority graph hover > source header > panel hover.
    if matches!(key.code, crossterm::event::KeyCode::Char('e'))
        && key.modifiers.is_empty()
        && app.patch.is_some()
    {
        // 1) Graph hovered node -> Circuit
        if let Some(idx) = app.hovered_graph_node {
            if let Some(graph) = app.graph.as_ref() {
                if let Some(node) = graph.nodes.get(idx).cloned() {
                    let draft = app
                        .current_patch_path
                        .as_ref()
                        .and_then(|p| app.label_store.circuit_label(p, &node.id))
                        .unwrap_or_default();
                    app.editing = Some(crate::app::EditState::new_circuit(node.id.clone(), draft));
                    let s = crate::config::load(
                        &crate::theme::canonical_theme_name,
                        crate::theme::THEMES,
                    );
                    if let Some(line) =
                        app.editing_status_line(s.labels.layers_enabled, s.labels.max_shift_layer)
                    {
                        app.status_message = line;
                    } else {
                        app.status_message = format!("Editing circuit {}:{}", node.id.0, node.id.1);
                    }
                    return false;
                }
            }
        }
        // Also check filtered graph when quad filtered focused
        if app.showing_quad {
            if let Some(fg) = app.filtered_graph.as_ref() {
                // Use filtered_positions hit? For now reuse hovered_graph_node for full; filtered drag has no hover index.
                // Fall through to source/panel if no full hover.
                let _ = fg;
            }
        }
        // 2) Source header focused -> Circuit instance at source_scroll
        let source_focused = (app.showing_viewer && app.viewer_focus == ViewerFocus::Source)
            || (app.showing_quad && app.quad_focus == QuadFocus::Source);
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
                    let mut node: Option<(String, usize)> = None;
                    for (i, sec) in patch.sections.iter().enumerate() {
                        let entry = counts.entry(sec.name.clone()).or_insert(0);
                        if i == idx {
                            node = Some((name.clone(), *entry));
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
                            app.status_message = format!("Editing circuit {}:{}", nid.0, nid.1);
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

    // Quad concurrent view handling — picker and prefix remain highest priority.
    if app.showing_quad {
        match key.code {
            crossterm::event::KeyCode::Esc => {
                app.close_quad();
                app.prefix = None;
                return false;
            }
            crossterm::event::KeyCode::Tab => {
                app.cycle_quad_focus();
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
                app.toggle_processing_pause();
                return false;
            }
            crossterm::event::KeyCode::Char('t') if app.quad_focus == QuadFocus::Source => {
                app.source_view_mode = match app.source_view_mode {
                    SourceViewMode::Raw => SourceViewMode::Prettified,
                    SourceViewMode::Prettified => SourceViewMode::Raw,
                };
                return false;
            }
            crossterm::event::KeyCode::Char('[') | crossterm::event::KeyCode::Char(']')
                if app.quad_focus == QuadFocus::Source =>
            {
                let delta = if matches!(key.code, crossterm::event::KeyCode::Char('[')) {
                    -0.1
                } else {
                    0.1
                };
                app.adjust_viewer_split_ratio(delta);
                app.viewer_split_ratio = (app.viewer_split_ratio * 10.0).round() / 10.0;
                let pct_panels = app.viewer_split_ratio * 100.0;
                let pct_source = 100.0 - pct_panels;
                app.status_message =
                    format!("Panels/Source split: {:.0}%/{:.0}%", pct_panels, pct_source);
                return false;
            }
            _ => {}
        }
        // Source-pane navigation when quad focus is on Source — mirrors the
        // embedded viewer's Source focus handling (Up/Down/Home/End only).
        if app.quad_focus == QuadFocus::Source {
            match key.code {
                crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                    if app.selected_component.is_some() {
                        let next = app.occurrence_cursor.saturating_add(1);
                        app.jump_to_occurrence(next);
                    }
                    return false;
                }
                crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
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
                _ => {}
            }
        }
        // Fall through to normal panel handling for Panels/Graph* focuses so
        // shift/scale/orientation/toggle still work while quad is open.
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
                app.toggle_processing_pause();
                return false;
            }
            crossterm::event::KeyCode::Char('c') => {
                // Bare `c` toggles cable latency coloring on the graph surface;
                // Ctrl+C (quit) is matched above with its modifier guard.
                app.toggle_latency_coloring();
                return false;
            }
            crossterm::event::KeyCode::Char('x') => {
                let Some(idx) = app.hovered_graph_node else {
                    return false;
                };
                let Some(node) = app.graph.as_ref().and_then(|g| g.nodes.get(idx)).cloned() else {
                    return false;
                };
                let now_disabled =
                    app.toggle_circuit_processing(&node.circuit, node.instance_index);
                app.rebuild_graph();
                if now_disabled {
                    app.status_message = format!(
                        "Processing disabled: {} {}",
                        node.circuit, node.instance_index
                    );
                } else {
                    app.status_message = format!(
                        "Processing enabled: {} {}",
                        node.circuit, node.instance_index
                    );
                }
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
                let delta = if matches!(key.code, crossterm::event::KeyCode::Char('[')) {
                    -0.1
                } else {
                    0.1
                };
                app.adjust_viewer_split_ratio(delta);
                // Snap to a clean 0.1 step so repeated presses stay exact
                // (avoids float drift such as 0.7000000000000001).
                app.viewer_split_ratio = (app.viewer_split_ratio * 10.0).round() / 10.0;
                let pct_panels = app.viewer_split_ratio * 100.0;
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
                        app.status_message = format!("Editing circuit {}:{}", node.id.0, node.id.1);
                    }
                    return false;
                }
            }
        }
        // Source header (quad or viewer) — resolve to section instance at source focus.
        if (app.showing_viewer && app.viewer_focus == ViewerFocus::Source)
            || (app.showing_quad && app.quad_focus == QuadFocus::Source)
        {
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
                    let nid = (section.name.clone(), *entry);
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
                            app.status_message = format!("Editing circuit {}:{}", nid.0, nid.1);
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
            // Cycle through the scaling presets defined by the module-scaling spec.
            const PRESETS: [f32; 4] = [0.5, 1.0, 1.5, 2.0];
            let idx = PRESETS
                .iter()
                .position(|p| (p - app.scale_factor).abs() < f32::EPSILON)
                .unwrap_or(1);
            let step = if matches!(key.code, crossterm::event::KeyCode::Char('+')) {
                1
            } else {
                PRESETS.len() - 1
            };
            let next = PRESETS[(idx + step) % PRESETS.len()];
            app.scale_factor = next;
            app.status_message = format!("Scaling: {}%", (next * 100.0) as u32);
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
    if app.editing.is_some() {
        return;
    }
    if app.showing_picker {
        return;
    }
    // While the quad view is open both graph panes own mouse drag input.
    // A drag started in one pane continues until Up regardless of hit.
    if app.showing_quad && handle_quad_mouse(mouse, app) {
        return;
    }
    // While the graph surface is open it owns all mouse input; nothing falls
    // through to panel/minimap handling below.
    if app.showing_graph {
        handle_graph_mouse(mouse, app);
        return;
    }
    // Minimap click-to-scroll: uses renderer-published minimap geometry with
    // the same proportional mapping as the viewport indicator in ui.rs
    // (indicator: scroll * inner_h / total_lines). Click must work whenever
    // the embedded viewer or quad is visible, regardless of focus, and takes
    // precedence over panel interactions (picker already returned above).
    if app.showing_viewer || app.showing_quad {
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
                // focus back to the panels while the viewer/quad stays open.
                if app.showing_quad {
                    app.quad_focus = QuadFocus::Panels;
                } else if app.showing_viewer {
                    app.viewer_focus = ViewerFocus::Panels;
                }
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
                let in_source_pane = (app.showing_viewer || app.showing_quad)
                    && !on_minimap
                    && app
                        .source_pane_rect
                        .is_some_and(|rect| rect_contains(&rect, mouse.column, mouse.row));
                if in_source_pane {
                    if app.showing_quad {
                        app.quad_focus = QuadFocus::Source;
                    } else {
                        app.viewer_focus = ViewerFocus::Source;
                    }
                } else {
                    if !on_minimap {
                        app.clear_selected_component();
                    }
                    if app.showing_quad {
                        app.quad_focus = QuadFocus::Panels;
                    } else if app.showing_viewer {
                        app.viewer_focus = ViewerFocus::Panels;
                    }
                }
            }
        }
        MouseEventKind::ScrollUp => {
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
        MouseEventKind::ScrollDown => {
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
            layout::local_resettle(
                graph,
                &mut app.graph_positions,
                &node_id,
                layout::LOCAL_RADIUS,
                layout::LOCAL_ITERATIONS,
            );
            app.notify_node_moved(&node_id);
        }
        MouseEventKind::Up(_) => {
            app.graph_drag = None;
        }
        _ => {}
    }
}

/// Handle mouse for the quad concurrent view's two graph panes.
/// Returns true if the event was consumed as a graph drag (Down on a node,
/// Drag while a grab is active, or Up releasing a grab).
fn handle_quad_mouse(mouse: MouseEvent, app: &mut App) -> bool {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Prioritize filtered pane hit, then full pane. Both use the same
            // renderer-published rect contract; quad_focus tracks selection.
            if let Some(idx) = app
                .filtered_node_rects
                .iter()
                .find(|(_, rect)| rect_contains(rect, mouse.column, mouse.row))
                .map(|(i, _)| *i)
            {
                app.quad_focus = QuadFocus::GraphFiltered;
                if let Some((px, py)) = app.filtered_positions.get(idx).copied() {
                    app.filtered_drag = Some(GraphDrag {
                        node_index: idx,
                        offset_x: px - mouse.column as f32,
                        offset_y: py - mouse.row as f32,
                    });
                }
                return true;
            }
            if let Some(idx) = app
                .graph_node_rects
                .iter()
                .find(|(_, rect)| rect_contains(rect, mouse.column, mouse.row))
                .map(|(i, _)| *i)
            {
                app.quad_focus = QuadFocus::GraphFull;
                if let Some((px, py)) = app.graph_positions.get(idx).copied() {
                    app.graph_drag = Some(GraphDrag {
                        node_index: idx,
                        offset_x: px - mouse.column as f32,
                        offset_y: py - mouse.row as f32,
                    });
                }
                return true;
            }
            false
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(drag) = app.filtered_drag.as_ref() {
                let node_index = drag.node_index;
                let offset_x = drag.offset_x;
                let offset_y = drag.offset_y;
                let filtered = app.filtered_graph.clone();
                let Some(graph) = filtered else {
                    return true;
                };
                let Some(node_id) = graph.nodes.get(node_index).map(|n| n.id.clone()) else {
                    return true;
                };
                if let Some(pos) = app.filtered_positions.get_mut(node_index) {
                    *pos = (
                        clamp_drag(mouse.column as f32 + offset_x),
                        clamp_drag(mouse.row as f32 + offset_y),
                    );
                }
                layout::local_resettle(
                    &graph,
                    &mut app.filtered_positions,
                    &node_id,
                    layout::LOCAL_RADIUS,
                    layout::LOCAL_ITERATIONS,
                );
                app.notify_node_moved(&node_id);
                return true;
            }
            if let Some(drag) = app.graph_drag.as_ref() {
                let node_index = drag.node_index;
                let offset_x = drag.offset_x;
                let offset_y = drag.offset_y;
                let graph = app.graph.clone();
                let Some(graph) = graph else {
                    return true;
                };
                let Some(node_id) = graph.nodes.get(node_index).map(|n| n.id.clone()) else {
                    return true;
                };
                if let Some(pos) = app.graph_positions.get_mut(node_index) {
                    *pos = (
                        clamp_drag(mouse.column as f32 + offset_x),
                        clamp_drag(mouse.row as f32 + offset_y),
                    );
                }
                layout::local_resettle(
                    &graph,
                    &mut app.graph_positions,
                    &node_id,
                    layout::LOCAL_RADIUS,
                    layout::LOCAL_ITERATIONS,
                );
                app.notify_node_moved(&node_id);
                return true;
            }
            false
        }
        MouseEventKind::Up(_) => {
            let was_dragging = app.filtered_drag.is_some() || app.graph_drag.is_some();
            app.filtered_drag = None;
            app.graph_drag = None;
            was_dragging
        }
        MouseEventKind::Moved => {
            // Hover does not consume in quad; let panel hover run.
            false
        }
        _ => false,
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
                } else if app.diff_picker_active {
                    match app.load_diff_patch(&selected_path) {
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
                            app.selected_file = Some(selected_path);
                            app.showing_picker = false;
                            app.diff_picker_active = false;
                        }
                        Err(e) => {
                            app.status_message = format!("Failed to load diff patch: {}", e);
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;
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
    fn plus_and_minus_cycle_scale_presets_with_status() {
        let mut app = App::new();
        // From the 100% default, '-' steps down one preset to 50%.
        handle_event(key(crossterm::event::KeyCode::Char('-')), &mut app);
        assert_eq!(app.scale_factor, 0.5);
        assert_eq!(app.status_message, "Scaling: 50%");

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
        assert_eq!(app.scale_factor, 0.5);
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
    fn p_toggles_pause_while_graph_open() {
        let mut app = app_with_fixture();
        app.open_graph();
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        assert!(app.processing_paused, "p live on the graph surface");
        assert!(app.showing_graph);
        assert_eq!(app.status_message, "Processing paused (p to resume)");
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
                .contains(&(node.circuit.clone(), node.instance_index)),
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
            .contains(&(node.circuit.clone(), node.instance_index)));
        // Second x on same hovered node re-enables.
        handle_event(key(crossterm::event::KeyCode::Char('x')), &mut app);
        assert!(
            !app.disabled_circuits
                .contains(&(node.circuit.clone(), node.instance_index)),
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
    fn graph_p_still_toggles_pause_while_graph_open() {
        let mut app = app_with_graph();
        assert!(!app.processing_paused);
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        assert!(
            app.processing_paused,
            "p must toggle pause on graph surface"
        );
        assert_eq!(app.status_message, "Processing paused (p to resume)");
        assert!(app.showing_graph, "p must not close the graph");
        handle_event(key(crossterm::event::KeyCode::Char('p')), &mut app);
        assert!(!app.processing_paused);
        assert_eq!(app.status_message, "Processing enabled (p to pause)");
    }
}
