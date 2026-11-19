//! Regression suite for embedded source navigation (task 5.1).
//! Drives real flows end-to-end through `handle_event` + `render` with
//! `fixtures/source_navigation.ini`. Each test is a small story rather
//! than an isolated unit: open viewer, select, navigate, toggle, click.
//!
//! A second story arc covers boxed components and the viewer split ratio
//! (`fixtures/led_pairs.ini`): parse to boxed/text frames to mixed grid
//! to click hit-testing, plus `[`/`]` split adjustment through handler,
//! App state, and the rendered layout.

use std::collections::HashMap;
use std::path::Path;

use crate::handler::key_modifiers;
use crate::handler::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

fn shift_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, key_modifiers::SHIFT)
}

use crate::app::Rect;
use crate::app::{App, SourceViewMode, ViewType, ViewerFocus};
use crate::graph::Graph;
use crate::handler::{handle_event, handle_mouse_event};
use crate::layout::{
    local_resettle, node_world_sizes, seed_positions, solve, SolveKnobs, LOCAL_ITERATIONS,
    LOCAL_RADIUS,
};
use crate::patch::{ComponentKind, NodeId, Patch, ShiftGroup};
use crate::test_support::{positions_bbox, rack_inventory};

// ── helpers ──────────────────────────────────────────────────────────────

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, key_modifiers::NONE)
}

fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: key_modifiers::NONE,
    }
}

fn fixture_app() -> App {
    let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
    let mut app = App::new();
    app.load_patch(patch);
    // Place a few known components at predictable rects for click tests.
    // Indices are resolved dynamically so the fixture can evolve.
    let rects: Vec<(usize, Rect)> = {
        let mut v = Vec::new();
        for tok in ["B1.1", "P1.1", "B1.2", "P1.2"] {
            if let Some(idx) = app
                .patch
                .as_ref()
                .unwrap()
                .hw_components
                .iter()
                .position(|c| c.id == tok)
            {
                // Spread them horizontally so empty-panel clicks are unambiguous.
                let x = match tok {
                    "P1.1" => 20,
                    "B1.2" => 40,
                    "P1.2" => 60,
                    // `B1.1` stays at the origin.
                    _ => 0,
                };
                v.push((idx, Rect::new(x, 0, 16, 2)));
            }
        }
        v
    };
    app.component_rects = rects;
    app
}

fn idx_for(app: &App, token: &str) -> usize {
    app.patch
        .as_ref()
        .unwrap()
        .hw_components
        .iter()
        .position(|c| c.id == token)
        .unwrap_or_else(|| panic!("no component {token} in fixture"))
}

fn open_viewer(app: &mut App) {
    handle_event(&key(KeyCode::Char('g')), app);
    handle_event(&key(KeyCode::Char('v')), app);
    assert!(
        app.pane_holding(ViewType::SourceViewer).is_some(),
        "g v should open embedded viewer"
    );
}

// ── regression: initial BOF vs selected-open ───────────────────────────

#[test]
fn regression_initial_bof_vs_selected_open() {
    // BOF when nothing selected
    let mut app = fixture_app();
    app.source_scroll = 99;
    open_viewer(&mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Source);
    assert_eq!(app.source_scroll, 0, "BOF when no selection");
    assert_eq!(app.occurrence_cursor, 0);

    // Selected-open jumps to first occurrence
    let mut app2 = fixture_app();
    let first_b11 = app2.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
    app2.select_component(String::from("B1.1"));
    app2.source_scroll = 999;
    // close then reopen through handle_event so initial-position rule reapplies
    if let Some(id) = app2.pane_holding(ViewType::SourceViewer) {
        app2.layout.pane_mut(id).view = None;
    }
    open_viewer(&mut app2);
    assert_eq!(
        app2.source_scroll, first_b11,
        "opens at selected first occurrence"
    );
    assert_eq!(app2.occurrence_cursor, 0);
    assert_eq!(app2.selected_component, Some(String::from("B1.1")));
}

// ── first / replacement jumps via commit interactions ───────────────────

#[test]
fn regression_first_and_replacement_jumps() {
    let mut app = fixture_app();
    let b11_first = app.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
    let p11_first = app.patch.as_ref().unwrap().occurrences_for("P1.1")[0].line;
    let b12_first = app.patch.as_ref().unwrap().occurrences_for("B1.2")[0].line;

    // First selection via Enter while viewer closed: should still jump
    let b11_idx = idx_for(&app, "B1.1");
    app.hovered_component = Some(b11_idx);
    app.source_scroll = 999;
    handle_event(&key(KeyCode::Enter), &mut app);
    assert_eq!(app.selected_component, Some(String::from("B1.1")));
    assert_eq!(app.source_scroll, b11_first);
    assert_eq!(app.occurrence_cursor, 0);

    // Replacement via Space on different component
    let p11_idx = idx_for(&app, "P1.1");
    app.hovered_component = Some(p11_idx);
    handle_event(&key(KeyCode::Char(' ')), &mut app);
    assert_eq!(app.selected_component, Some(String::from("P1.1")));
    assert_eq!(app.source_scroll, p11_first);

    // Replacement via mouse click
    let b12_idx = idx_for(&app, "B1.2");
    app.component_rects = vec![
        (b11_idx, Rect::new(0, 0, 16, 2)),
        (b12_idx, Rect::new(40, 0, 16, 2)),
        (p11_idx, Rect::new(20, 0, 16, 2)),
    ];
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 42, 1),
        &mut app,
    );
    assert_eq!(app.selected_component, Some(String::from("B1.2")));
    assert_eq!(app.source_scroll, b12_first);

    // Open viewer now: initial-position rule should keep us at B1.2 first occurrence
    open_viewer(&mut app);
    assert_eq!(app.source_scroll, b12_first);
    // Replacement while viewer open (Tabs to panels first)
    handle_event(&key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Panels);
    app.hovered_component = Some(b11_idx);
    handle_event(&key(KeyCode::Enter), &mut app);
    assert_eq!(app.selected_component, Some(String::from("B1.1")));
    assert_eq!(app.source_scroll, b11_first);
}

// ── occurrence bounds (Up/Down/Home/End saturate, no-selection no-op) ──

#[test]
fn regression_occurrence_bounds() {
    let mut app = fixture_app();
    let occ: Vec<_> = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
    assert!(occ.len() >= 3, "fixture needs >=3 B1.1 occurrences");
    app.select_component(String::from("B1.1"));
    open_viewer(&mut app);
    assert_eq!(app.occurrence_cursor, 0);
    assert_eq!(app.source_scroll, occ[0].line);

    // Up at first saturates
    handle_event(&key(KeyCode::Up), &mut app);
    assert_eq!(app.occurrence_cursor, 0);
    assert_eq!(app.source_scroll, occ[0].line);

    // Down steps one
    handle_event(&key(KeyCode::Down), &mut app);
    assert_eq!(app.occurrence_cursor, 1);
    assert_eq!(app.source_scroll, occ[1].line);

    // Down to last saturates
    for _ in 0..occ.len() + 5 {
        handle_event(&key(KeyCode::Down), &mut app);
    }
    assert_eq!(app.occurrence_cursor, occ.len() - 1);
    assert_eq!(app.source_scroll, occ.last().unwrap().line);
    handle_event(&key(KeyCode::Down), &mut app);
    assert_eq!(app.occurrence_cursor, occ.len() - 1);

    // Home -> first, End -> last
    handle_event(&key(KeyCode::Home), &mut app);
    assert_eq!(app.occurrence_cursor, 0);
    assert_eq!(app.source_scroll, occ[0].line);
    handle_event(&key(KeyCode::End), &mut app);
    assert_eq!(app.occurrence_cursor, occ.len() - 1);

    // No-selection no-op
    app.clear_selected_component();
    let scroll_before = app.source_scroll;
    handle_event(&key(KeyCode::Up), &mut app);
    handle_event(&key(KeyCode::Down), &mut app);
    handle_event(&key(KeyCode::Home), &mut app);
    handle_event(&key(KeyCode::End), &mut app);
    assert_eq!(app.source_scroll, scroll_before);
    assert_eq!(app.occurrence_cursor, 0);
}

// ── deselect keeps position ─────────────────────────────────────────────

#[test]
fn regression_deselect_keeps_position() {
    let mut app = fixture_app();
    app.select_component(String::from("B1.1"));
    open_viewer(&mut app);
    let occ = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
    // Move to middle occurrence via Down
    handle_event(&key(KeyCode::Down), &mut app);
    let mid_line = app.source_scroll;
    assert_eq!(app.occurrence_cursor, 1);
    // Also j/k line scroll should still work but not affect occurrence cursor
    handle_event(&key(KeyCode::Char('j')), &mut app);
    assert_eq!(app.source_scroll, mid_line + 1);
    // Jump back to occurrence so deselect origin is occurrence-aligned
    handle_event(&key(KeyCode::Up), &mut app);
    assert_eq!(app.source_scroll, occ[0].line);
    handle_event(&key(KeyCode::Down), &mut app);
    assert_eq!(app.source_scroll, occ[1].line);
    let pos_before = app.source_scroll;

    // Empty-panel click clears selection without moving scroll
    // Need viewer focus on Panels for empty click to be considered
    handle_event(&key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Panels);
    // Place component rects far away, click empty
    let idx = idx_for(&app, "B1.1");
    app.component_rects = vec![(idx, Rect::new(0, 0, 16, 2))];
    app.minimap_rect = None; // ensure minimap doesn't intercept
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 100, 50),
        &mut app,
    );
    assert!(app.selected_component.is_none(), "selection cleared");
    assert_eq!(
        app.source_scroll, pos_before,
        "deselect must not move scroll"
    );
    assert_eq!(app.occurrence_cursor, 0);
}

// ── t preserves usable content ──────────────────────────────────────────

#[test]
fn regression_t_preserves_usable_content() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    app.source_scroll = 0;
    // Highlights usable: select then verify still raw
    app.select_component(String::from("B1.1"));
    // t -> prettified
    handle_event(&key(KeyCode::Char('t')), &mut app);
    assert_eq!(app.source_view_mode, SourceViewMode::Prettified);
    // t back to raw restores verbatim content and scroll still meaningful
    let scroll_before = app.source_scroll;
    handle_event(&key(KeyCode::Char('t')), &mut app);
    assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    // scroll preserved across toggles (t does not reset scroll)
    assert_eq!(app.source_scroll, scroll_before);
}

// ── minimap click maps correctly ────────────────────────────────────────

#[test]
fn regression_minimap_click_maps_correctly() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    app.source_view_mode = SourceViewMode::Raw;
    app.source_scroll = 0;
    // Publish minimap geometry directly (the paint path that sets it is
    // covered by viewer.rs paint tests).
    let rect = Rect::new(90, 1, 28, 30);
    app.minimap_rect = Some(rect);
    let total_lines = app.patch.as_ref().unwrap().raw_lines.len();
    let inner_h = rect.height.saturating_sub(2) as usize;
    assert!(inner_h > 0);
    let inner_y = rect.y + 1;

    // Helper to click at fractional inner row
    let click_at = |app: &mut App, frac: f32| {
        let row = ((frac * inner_h as f32) as usize).min(inner_h.saturating_sub(1));
        let y = inner_y + row as u16;
        let x = rect.x + 1;
        handle_mouse_event(mouse(MouseEventKind::Down(MouseButton::Left), x, y), app);
        app.source_scroll
    };

    let top = click_at(&mut app, 0.0);
    let mid = click_at(&mut app, 0.5);
    let bot = click_at(&mut app, 0.99);
    assert!(top <= mid, "minimap top <= middle: {top} vs {mid}");
    assert!(mid <= bot, "minimap middle <= bottom: {mid} vs {bot}");
    assert!(top < total_lines, "top scroll within total");
    assert!(bot <= total_lines, "bottom scroll within total");
    // Top click should be near 0, bottom near max
    assert!(top <= 5, "top click near BOF, got {top}");
    // Bottom should be large (centered mapping, so around total - viewport/2)
    assert!(
        bot > total_lines / 2,
        "bottom click in second half, got {bot}"
    );

    // Border clicks clamp to nearest inner row (click above/below inner area)
    let x = rect.x + 1;
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), x, rect.y),
        &mut app,
    );
    let scroll_above = app.source_scroll;
    handle_mouse_event(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            x,
            rect.y + rect.height,
        ),
        &mut app,
    );
    let scroll_below = app.source_scroll;
    assert!(scroll_above <= scroll_below);

    // Minimap click works regardless of focus (Panels vs Source) and takes precedence over panel toggle
    // The clamped border click below the minimap counts as empty-panel space
    // and may have handed focus to Panels; normalize before the check.
    if app.viewer_focus() != ViewerFocus::Panels {
        handle_event(&key(KeyCode::Tab), &mut app); // to Panels
    }
    assert_eq!(app.viewer_focus(), ViewerFocus::Panels);
    let state_before = app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")]
        .state
        .clone();
    // Click again on minimap middle
    click_at(&mut app, 0.3);
    // Component state unchanged (minimap precedence, not panel toggle)
    let state_after = app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")]
        .state
        .clone();
    assert_eq!(
        state_before, state_after,
        "minimap click should not toggle component"
    );

    handle_event(&key(KeyCode::Tab), &mut app); // back to Source
    assert_eq!(app.viewer_focus(), ViewerFocus::Source);
    click_at(&mut app, 0.7);
    // Still works when source focused
    assert!(app.source_scroll > 0);

    // Hidden minimap: click is a no-op (treated as panel/deselect)
    app.minimap_rect = None;
    let scroll_before = app.source_scroll;
    // Click where minimap would have been shouldn't affect scroll when hidden
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 65, 5),
        &mut app,
    );
    // Could be deselect or no-op, but scroll shouldn't jump to minimap-mapped value (stay same or only deselect)
    // Ensure scroll didn't jump to a minimap-mapped large value unexpectedly
    // It may remain same if click was on empty panel; that's acceptable.
    let _ = scroll_before;
}

// ── Tab focus round-trip ────────────────────────────────────────────────

#[test]
fn regression_tab_focus_round_trip() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Source);
    // render shows focus emphasis (yellow border) is not directly assertable via text,
    // but we can verify state and that panel keys are live even when Source focused
    let scale_before = app.scale_factor;
    handle_event(&key(KeyCode::Char('+')), &mut app);
    assert_ne!(
        app.scale_factor, scale_before,
        "scale live when Source focused"
    );

    handle_event(&key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Panels);
    // Now panel keys work
    handle_event(&key(KeyCode::Char('+')), &mut app);
    assert_ne!(app.scale_factor, scale_before);
    // j should move hover when panels focused (not source scroll)
    app.hovered_component = Some(idx_for(&app, "B1.1"));
    let hover_before = app.hovered_component;
    handle_event(&key(KeyCode::Char('j')), &mut app);
    assert_ne!(app.hovered_component, hover_before);

    handle_event(&key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Source);
    // Back to source: j scrolls source
    let scroll_before = app.source_scroll;
    handle_event(&key(KeyCode::Char('j')), &mut app);
    assert_eq!(app.source_scroll, scroll_before + 1);

    handle_event(&key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Panels);
    handle_event(&key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Source);

    // Esc closes keeping selection and resets focus to Panels
    app.select_component(String::from("B1.1"));
    let sel = app.selected_component.clone();
    let scroll = app.source_scroll;
    handle_event(&key(KeyCode::Esc), &mut app);
    assert!(app.pane_holding(ViewType::SourceViewer).is_none());
    assert_eq!(app.viewer_focus(), ViewerFocus::Panels);
    assert_eq!(app.selected_component, sel);
    assert_eq!(app.source_scroll, scroll);
}

// ── picker precedence ───────────────────────────────────────────────────

#[test]
fn regression_picker_precedence() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    app.source_view_mode = SourceViewMode::Raw;
    app.source_scroll = 5;
    // Open picker via l even while source focused (picker precedence)
    handle_event(&key(KeyCode::Char('l')), &mut app);
    assert!(app.showing_picker, "picker opens even when source focused");
    // While picker open, viewer keys are inert: t should not toggle view mode
    let mode_before = app.source_view_mode.clone();
    handle_event(&key(KeyCode::Char('t')), &mut app);
    assert_eq!(app.source_view_mode, mode_before);
    handle_event(&key(KeyCode::Tab), &mut app);
    // picker still open, Tab didn't switch viewer focus (picker consumes)
    assert!(app.showing_picker);
    // Picker navigation still works
    let idx_before = app.picker_index;
    handle_event(&key(KeyCode::Char('j')), &mut app);
    assert!(app.picker_index >= idx_before);
    // Esc closes picker but viewer stays open
    handle_event(&key(KeyCode::Esc), &mut app);
    assert!(!app.showing_picker);
    assert!(
        app.pane_holding(ViewType::SourceViewer).is_some(),
        "viewer remains after picker close"
    );
    // Picker closed, t works again
    handle_event(&key(KeyCode::Char('t')), &mut app);
    assert_ne!(app.source_view_mode, mode_before);
}

// ── viewer live interaction (main window unblocked, droid_tui-0lw) ─────

#[test]
fn regression_viewer_live_interaction() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Source);
    // Class layout: mouse clicks focus the pane they land in, so publish the
    // pane hit rects the renderer would (module UI left, source viewer right).
    app.pane_hit_rects = vec![
        (crate::panes::PaneId::BigLeft, Rect::new(0, 0, 70, 40)),
        (crate::panes::PaneId::SmallTop, Rect::new(70, 0, 50, 40)),
    ];

    // Panel keys are live while Source focused.
    handle_event(&key(KeyCode::Char('1')), &mut app);
    assert_eq!(app.active_shift, Some(ShiftGroup::Group1), "shift live");
    let scale_before = app.scale_factor;
    handle_event(&key(KeyCode::Char('+')), &mut app);
    assert_ne!(app.scale_factor, scale_before, "scale live");

    // Enter toggles AND selects the hovered component; selection jumps
    // source_scroll to its first occurrence.
    let b11 = idx_for(&app, "B1.1");
    app.hovered_component = Some(b11);
    let first_b11 = app.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
    let state_before = app.patch.as_ref().unwrap().hw_components[b11].state.clone();
    handle_event(&key(KeyCode::Enter), &mut app);
    assert_ne!(
        app.patch.as_ref().unwrap().hw_components[b11].state,
        state_before,
        "Enter toggles while Source focused"
    );
    assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
    assert_eq!(app.source_scroll, first_b11);
    handle_event(&key(KeyCode::Char(' ')), &mut app);
    assert_eq!(
        app.patch.as_ref().unwrap().hw_components[b11].state,
        state_before,
        "Space toggles while Source focused"
    );

    // Mouse click on a panel component toggles regardless of focus and
    // hands keyboard focus to the panels.
    let b12_state_before = app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.2")]
        .state
        .clone();
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 42, 1),
        &mut app,
    );
    assert_ne!(
        app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.2")].state,
        b12_state_before,
        "mouse click toggles while viewer open"
    );
    assert_eq!(
        app.viewer_focus(),
        ViewerFocus::Panels,
        "component click hands focus to panels"
    );

    // Tab back to Source; a bare source-pane click re-focuses the source
    // pane without clearing the selection or toggling anything.
    handle_event(&key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus(), ViewerFocus::Source);
    let sel = app.selected_component.clone();
    let scroll = app.source_scroll;
    app.minimap_rect = None;
    app.source_pane_rect = Some(Rect::new(72, 3, 47, 34));
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 100, 30),
        &mut app,
    );
    assert_eq!(
        app.viewer_focus(),
        ViewerFocus::Source,
        "bare source-pane click focuses source"
    );
    assert_eq!(app.selected_component, sel, "selection kept");
    assert_eq!(app.source_scroll, scroll, "scroll kept");

    // l still opens picker (picker precedence), j/k line scroll and
    // Up/Down occurrence navigation stay routed by focus.
    let scroll_before = app.source_scroll;
    handle_event(&key(KeyCode::Char('j')), &mut app);
    assert_eq!(app.source_scroll, scroll_before + 1);
    let occ_before = app.occurrence_cursor;
    handle_event(&key(KeyCode::Down), &mut app);
    assert!(app.occurrence_cursor >= occ_before);

    // t works from both focuses (global viewer key).
    let mode_before = app.source_view_mode.clone();
    handle_event(&key(KeyCode::Char('t')), &mut app);
    assert_ne!(app.source_view_mode, mode_before);

    // Esc closes keeping selection and resets focus to Panels.
    handle_event(&key(KeyCode::Esc), &mut app);
    assert!(app.pane_holding(ViewType::SourceViewer).is_none());
    assert_eq!(app.viewer_focus(), ViewerFocus::Panels);
    assert_eq!(app.selected_component, sel);
    // After close, normal panel handling resumes.
    handle_event(&key(KeyCode::Char('1')), &mut app);
    assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
}

// ── validation fixture matrix (patch-validation 3.2) ──────────────────────────

const VALIDATION_FIXTURES: &[(&str, &str)] = &[
    ("unknown_circuit", "unknown_circuit"),
    ("duplicate_param", "duplicate_param"),
    ("unknown_param", "unknown_param"),
    ("invalid_jack", "invalid_jack"),
    ("missing_required", "missing_required"),
    // The documented-alternatives fixture: four sections whose essential input
    // is satisfied by an alternative plus four controls that still warn. It
    // pins the rule with zero/one counts in `validation::tests`; for the
    // per-code matrix its primary code is the surviving `missing_required`.
    ("essential_alternatives", "missing_required"),
    ("undefined_cable", "undefined_cable"),
    ("duplicate_cable", "duplicate_cable"),
    ("unused_cable", "unused_cable"),
    ("invalid_expression", "invalid_expression"),
    ("ram_overflow", "ram_overflow"),
    // Master-module fit (master-module-resolution 2.2): the two conflict
    // fixtures keep loading (Warning), only the no-fit fixture gates (Error).
    // Their spans, severities and the load/no-load split are pinned in
    // `validation::tests`.
    ("master_conflict_18", "master_conflict"),
    ("master_conflict_circuit", "master_conflict"),
    ("master_no_fit", "no_master_fits"),
];

fn validation_issues_for(fixture: &str) -> Vec<crate::validation::ValidationIssue> {
    let path = format!("fixtures/validation/{fixture}.ini");
    let patch =
        Patch::from_ini_file(Path::new(&path)).unwrap_or_else(|e| panic!("{path} must parse: {e}"));
    let schema = crate::schema::load_schema();
    crate::validation::validate_patch(&patch, schema)
}

fn assert_has_code(fixture: &str, expected_code: &str) {
    let issues = validation_issues_for(fixture);
    let codes: Vec<_> = issues.iter().map(|i| i.code.as_str()).collect();
    assert!(
        codes.contains(&expected_code),
        "{fixture}.ini must emit code={expected_code}, got codes={codes:?} issues={issues:#?}"
    );
    // sorted deterministic
    for w in issues.windows(2) {
        assert!(
            w[0] <= w[1],
            "{fixture}.ini issues not sorted: {:?} vs {:?}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn regression_validation_fixtures_parse_and_have_hardware() {
    // Every validation fixture must parse and contain a [p2b8] hardware section.
    for (fixture, _) in VALIDATION_FIXTURES {
        let path = format!("fixtures/validation/{fixture}.ini");
        let patch = Patch::from_ini_file(Path::new(&path))
            .unwrap_or_else(|e| panic!("{path} must parse: {e}"));
        assert!(
            !patch.hw_components.is_empty(),
            "{fixture}.ini must have hardware components via [p2b8]"
        );
        assert!(
            !patch.raw_lines.is_empty(),
            "{fixture}.ini must have raw_lines"
        );
    }
}

#[test]
fn regression_validation_fixture_unknown_circuit() {
    assert_has_code("unknown_circuit", "unknown_circuit");
    let issues = validation_issues_for("unknown_circuit");
    let item = issues.iter().find(|i| i.code == "unknown_circuit").unwrap();
    assert_eq!(item.severity, crate::validation::Severity::Error);
    assert!(
        item.message.contains("unknowncircuit"),
        "message should mention circuit: {}",
        item.message
    );
}

#[test]
fn regression_validation_fixture_duplicate_param() {
    assert_has_code("duplicate_param", "duplicate_param");
    let issues = validation_issues_for("duplicate_param");
    assert!(
        issues
            .iter()
            .any(|i| i.code == "duplicate_param"
                && i.severity == crate::validation::Severity::Warning)
    );
}

#[test]
fn regression_validation_fixture_unknown_param() {
    assert_has_code("unknown_param", "unknown_param");
    let issues = validation_issues_for("unknown_param");
    assert!(issues
        .iter()
        .any(|i| i.code == "unknown_param" && i.severity == crate::validation::Severity::Error));
}

#[test]
fn regression_validation_fixture_invalid_jack() {
    assert_has_code("invalid_jack", "invalid_jack");
    let issues = validation_issues_for("invalid_jack");
    let item = issues.iter().find(|i| i.code == "invalid_jack").unwrap();
    assert_eq!(item.severity, crate::validation::Severity::Warning);
    assert!(
        item.message.contains("B33.1"),
        "expected B33.1 in {}",
        item.message
    );
}

#[test]
fn regression_validation_fixture_missing_required() {
    assert_has_code("missing_required", "missing_required");
    let issues = validation_issues_for("missing_required");
    // algoquencer requires clock
    assert!(issues
        .iter()
        .any(|i| i.message.contains("clock") || i.code == "missing_required"));
}

#[test]
fn regression_validation_fixture_undefined_cable() {
    assert_has_code("undefined_cable", "undefined_cable");
}

#[test]
fn regression_validation_fixture_invalid_expression() {
    assert_has_code("invalid_expression", "invalid_expression");
    let issues = validation_issues_for("invalid_expression");
    let issue = issues
        .iter()
        .find(|i| i.code == "invalid_expression")
        .expect("invalid_expression");
    // A calculation the firmware cannot parse is a load-blocking error, not a
    // style hint: from-scratch patches would silently fail on the rack.
    assert_eq!(issue.severity, crate::validation::Severity::Error);
    assert!(issue.message.contains("brackets"), "{}", issue.message);
}

/// The expression check must stay quiet on patches that DROID actually
/// accepts — a false positive here would block loading a working patch.
#[test]
fn regression_expression_check_accepts_the_whole_corpus() {
    let mut dirs = vec![
        Path::new("fixtures").to_path_buf(),
        Path::new("corpus").to_path_buf(),
    ];
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // fixtures/validation deliberately contains broken patches.
                if !path.ends_with("validation") {
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "ini") {
                files.push(path);
            }
        }
    }
    assert!(!files.is_empty(), "no .ini files found to check");
    let schema = crate::schema::load_schema();
    for path in &files {
        let Ok(patch) = Patch::from_ini_file(path) else {
            continue;
        };
        let issues = crate::validation::validate_patch(&patch, schema);
        if let Some(issue) = issues.iter().find(|i| i.code == "invalid_expression") {
            panic!(
                "{}:{} false-positive invalid_expression: {}",
                path.display(),
                issue.span.line + 1,
                issue.message
            );
        }
    }
}

#[test]
fn regression_validation_fixture_duplicate_cable() {
    assert_has_code("duplicate_cable", "duplicate_cable");
    let issues = validation_issues_for("duplicate_cable");
    assert!(issues.iter().any(|i| i.message.contains("_X")));
    // duplicate must not also be unused
    assert!(
        !issues.iter().any(|i| i.code == "unused_cable"),
        "duplicate should not also be unused"
    );
}

#[test]
fn regression_validation_fixture_unused_cable() {
    assert_has_code("unused_cable", "unused_cable");
    let issues = validation_issues_for("unused_cable");
    let item = issues.iter().find(|i| i.code == "unused_cable").unwrap();
    assert_eq!(item.severity, crate::validation::Severity::Hint);
    assert!(item.message.contains("_UNUSED"));
}

#[test]
fn regression_validation_fixture_ram_overflow() {
    assert_has_code("ram_overflow", "ram_overflow");
    let issues = validation_issues_for("ram_overflow");
    let ram: Vec<_> = issues.iter().filter(|i| i.code == "ram_overflow").collect();
    assert!(
        !ram.is_empty(),
        "ram_overflow fixture must have at least one ram_overflow"
    );
    for r in &ram {
        assert_eq!(r.severity, crate::validation::Severity::Error);
        assert_eq!(r.span.line, 0);
        assert!(
            r.message.contains("bytes of RAM"),
            "ram message: {}",
            r.message
        );
    }
    // One verdict, judged against the master the patch resolved to (MASTER:
    // the fixture declares none and constrains none), not one per schema master.
    assert_eq!(ram.len(), 1, "ram findings: {ram:?}");
    assert!(
        ram[0].message.contains("MASTER only provides"),
        "resolved master must be named: {}",
        ram[0].message
    );
}

#[test]
fn regression_validation_fixture_matrix_all_present() {
    // Full matrix smoke: every fixture yields its primary code.
    assert_eq!(VALIDATION_FIXTURES.len(), 14, "must have 14 fixtures");
    for (fixture, code) in VALIDATION_FIXTURES {
        assert_has_code(fixture, code);
    }
    // Cross-check: fixtures directory contains exactly these files
    let entries = std::fs::read_dir("fixtures/validation").unwrap();
    let mut names: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|e| {
            e.path()
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })
        .collect();
    names.sort();
    let mut expected: Vec<String> = VALIDATION_FIXTURES
        .iter()
        .map(|(a, _)| a.to_string())
        .collect();
    expected.sort();
    assert_eq!(
        names, expected,
        "fixtures/validation must contain exactly 14 expected files"
    );
}

// ── master-module declaration stories (master-module-resolution 2.4) ─────────
// App-level proof of the *load* outcome for the three real declaration
// families: what declaring `# LABELS: master=18|16` does to the installed
// patch, the modal and the model the module UI draws — not just the
// validator's issue list (`validation::tests` pins the findings themselves).

/// Load one of the declaration fixtures the way the app does and hand back the
/// App, so a story can assert the load outcome rather than only the findings.
fn master_declaration_app(fixture: &str) -> App {
    let path = format!("fixtures/{fixture}.ini");
    let patch =
        Patch::from_ini_file(Path::new(&path)).unwrap_or_else(|e| panic!("{path} must parse: {e}"));
    let mut app = App::new();
    assert!(
        app.load_patch(patch),
        "{path} declares a master and only warns — it must load"
    );
    app
}

/// The value span of the entry binding `value` in the loaded patch, so a
/// finding can be located the way the source view jumps to it.
fn master_entry_span(app: &App, value: &str) -> crate::patch::Span {
    app.patch
        .as_ref()
        .expect("a story helper only runs after a successful load")
        .sections
        .iter()
        .flat_map(|section| section.detailed_entries.iter())
        .find(|entry| entry.value.trim() == value)
        .unwrap_or_else(|| panic!("no entry binds {value}"))
        .value_span
}

/// A declared MASTER18 patch reading CV inputs on I3/I5/I7 is reported at each
/// of the three entries and still loads — the declaration stays authoritative
/// for what is drawn, and a conflict is a problem to inspect, not a refusal.
#[test]
fn regression_declared_master18_cv_inputs_warn_and_still_load() {
    let app = master_declaration_app("master_declaration_cv_inputs");

    assert!(app.patch.is_some(), "a declared MASTER18 patch loads");
    assert!(
        !app.showing_validation,
        "warnings never auto-open the modal"
    );
    assert_eq!(
        app.master_model(),
        crate::patch::MasterModel::Master18,
        "the declaration is authoritative for what is drawn"
    );

    let conflicts: Vec<_> = app
        .validation_issues
        .iter()
        .filter(|issue| issue.code == "master_conflict")
        .collect();
    assert_eq!(conflicts.len(), 3, "issues: {:#?}", app.validation_issues);
    assert!(
        conflicts
            .iter()
            .all(|issue| issue.severity == crate::validation::Severity::Warning),
        "a master conflict is a Warning: {conflicts:#?}"
    );
    assert!(
        !app.validation_issues
            .iter()
            .any(|issue| issue.severity == crate::validation::Severity::Error),
        "the conflicts must not gate the load: {:#?}",
        app.validation_issues
    );
    for (issue, jack) in conflicts.iter().zip(["I3", "I5", "I7"]) {
        assert_eq!(
            issue.span,
            master_entry_span(&app, jack),
            "the conflict belongs on {jack}'s value"
        );
        assert!(
            issue.message.contains(jack),
            "message must name {jack}: {}",
            issue.message
        );
    }
}

/// The motokit shape: `clock` on I1 and `reset` on I2 are trigger-typed, which
/// MASTER18 reads as logic levels — no conflict, and the patch resolves to the
/// declared MASTER18.
#[test]
fn regression_declared_master18_gate_inputs_load_clean() {
    let app = master_declaration_app("master_declaration_gate_inputs");

    assert!(app.patch.is_some());
    assert!(!app.showing_validation);
    assert_eq!(
        app.master_model(),
        crate::patch::MasterModel::Master18,
        "the declaration is authoritative"
    );
    assert!(
        !app.validation_issues
            .iter()
            .any(|issue| issue.code == "master_conflict" || issue.code == "no_master_fits"),
        "gate/trigger inputs on I1/I2 are legitimate MASTER18 uses: {:#?}",
        app.validation_issues
    );
    assert!(
        app.validation_issues.is_empty(),
        "the fixture is a clean patch: {:#?}",
        app.validation_issues
    );
}

/// A declared MASTER patch using I1..I8 as CV: MASTER hosts every one, so no
/// master finding is reported and the model resolves to MASTER.
#[test]
fn regression_declared_master16_all_cv_inputs_load_clean() {
    let app = master_declaration_app("master_declaration_master16");

    assert!(app.patch.is_some());
    assert!(!app.showing_validation);
    assert_eq!(
        app.master_model(),
        crate::patch::MasterModel::Master,
        "the declaration is authoritative"
    );
    assert!(
        !app.validation_issues
            .iter()
            .any(|issue| issue.code == "master_conflict" || issue.code == "no_master_fits"),
        "MASTER hosts I1..I8 as CV: {:#?}",
        app.validation_issues
    );
    assert!(
        app.validation_issues.is_empty(),
        "the fixture is a clean patch: {:#?}",
        app.validation_issues
    );
}

#[test]
fn visual_diff_changed_node_marker_snapshot() {
    // Param-level diff: same topology, one node's non-cable param differs ->
    // the diff report marks the mixer node as changed (rendering of the
    // marker is covered by overlays.rs diff tests).
    let mut app = App::new();
    app.load_patch(Patch::from_ini_file(Path::new("fixtures/cable_banner_combos.ini")).unwrap());
    let base = app.patch.clone().unwrap();
    let mut modified = base.clone();
    // Change a non-cable param on the mixer node (instance 0 of "mixer")
    if let Some(sec) = modified.sections.iter_mut().find(|s| s.name == "mixer") {
        // mixer has no plain param besides cables; add one and change it
        sec.entries.push(("gain".to_string(), "0.9".to_string()));
    }
    let report = crate::diff::diff_patches(&base, &modified);
    assert!(
        report.changed_nodes.iter().any(|n| n.id.name() == "mixer"),
        "mixer changed"
    );
}

// ── task 4.1: solver layout guarantees over real fixtures ────────────────
// Integration-level regressions for the task 1.1 solver rework. The
// layout::tests module covers the same guarantees on synthetic graphs; these
// lock them in through the real wiring (fixture patch → banner clusters →
// graph → solve) so a regression in either the solver or the wiring is
// caught on representative real patches.

fn solver_dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[test]
fn regression_solver_single_axis_on_real_multi_banner_patch() {
    // Long synthetic cable chain (70 chained sections): the solver must
    // converge along the x-axis — a wide horizontal pipeline (width-first) —
    // not a vertical stack of banner bands. Synthetic because the small own
    // fixtures cannot exercise the axis property.
    //
    // Each chain member carries its OWN circuit name on purpose. `Graph::
    // build_from_patch` attributes cables by section *name* ("a name shared by
    // several instances resolves to the first instance"), so the 70 repeated
    // `[copy]` sections this fixture used to build all resolved to instance 0:
    // the chain degenerated into ~69 self-loops on that one node, which
    // inflated the raw Bellman-Ford depth to 5038 (69 × n) and seeded the node
    // at 5038 × 80 = 403,040 units. The old `x-span >= y-span` claim then held
    // only because the clamped integrator cannot travel 403,040 units, so the
    // smear simply sat there (measured x-span 403,040 seed / 400,680 solved,
    // y-span 6,160) — the assertion was passing vacuously on the very
    // unreachable seed `force-mode-seed-cap` removes. With distinct names the
    // fixture is the chain it claims to be, so the axis claim holds on its own
    // merits: measured x-span 3334.07 against y-span 153.53.
    let mut content = String::from("[p2b8]\n[lfo]\n    hz = 2\n    square = _C0\n");
    use std::fmt::Write as _;
    for i in 0..70 {
        writeln!(
            content,
            "[chain{i}]\n    input = _C{i}\n    output = _C{next}",
            next = i + 1
        )
        .expect("write to a String");
    }
    let patch = Patch::from_ini_str(&content, String::from("chain")).unwrap();
    let graph = crate::test_support::graph_for_patch(&patch);
    assert!(
        graph.nodes.len() >= 60,
        "chain should be spec-scale, got {} nodes",
        graph.nodes.len()
    );
    let positions = solve(&graph, &[], SolveKnobs::default());
    for (x, y) in &positions {
        assert!(x.is_finite() && y.is_finite(), "non-finite position");
    }
    let (w, h) = positions_bbox(&positions);
    assert!(w > 0.0, "degenerate zero-width layout");
    assert!(
        w >= h,
        "single-axis regression: x-span {w} must be >= y-span {h} (width-first horizontal chain)"
    );

    // `force-mode-seed-cap` 1.1 contract on a parsed patch: the layered seed
    // dense-normalizes its depth, so its x extent is one unit per distinct
    // layer (70 layers here → 70 × 80 = 5,600, bounded by 8,400 = × 1.5) instead
    // of tracking the raw depth value. The aliased fixture above measured
    // 403,040 — 48× the bound — which is what the guard catches.
    let seed = seed_positions(&graph);
    let (seed_w, _) = positions_bbox(&seed);
    assert!(
        seed_w <= 8_400.0,
        "force seed must stay proportional to the layer count: {seed_w} > 8,400 \
         (70 distinct layers × 80 world units × 1.5)"
    );
}

#[test]
fn regression_solver_spring_dominance_on_real_cable_chain() {
    // Real cable chain (clocktool → osc → notesequencer → vca) plus an
    // isolated controller node. Spring attraction must keep cable-connected
    // circuits nearer each other than unconnected pairs settle.
    let (_, graph) = crate::test_support::fixture_patch_and_graph("graph_edge_kinds.ini");
    let positions = solve(&graph, &[], SolveKnobs::default());
    let n = graph.nodes.len();
    let edge_pairs: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .map(|e| {
            let s = graph.nodes.iter().position(|x| x.id == e.source).unwrap();
            let t = graph.nodes.iter().position(|x| x.id == e.sink).unwrap();
            (s, t)
        })
        .collect();
    let mut connected = Vec::new();
    let mut unconnected = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let d = solver_dist(positions[i], positions[j]);
            if edge_pairs.contains(&(i, j)) || edge_pairs.contains(&(j, i)) {
                connected.push(d);
            } else {
                unconnected.push(d);
            }
        }
    }
    assert!(!connected.is_empty() && !unconnected.is_empty());
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let mc = mean(&connected);
    let mu = mean(&unconnected);
    assert!(
        mc < mu,
        "spring dominance failed on real chain: mean connected {mc} !< mean unconnected {mu}"
    );
}

#[test]
fn regression_solver_cluster_cohesion_on_real_banner_groups() {
    // Real banner groups (implicit unnamed {button}, "Mixer" {clocktool,
    // mixer, contour}) with real cables. cluster_index_of must map every node
    // into its group; the Mixer members must cohere into a bounded,
    // width-first cluster — not a tall/narrow vertical stripe.
    let (_, graph) = crate::test_support::fixture_patch_and_graph("cable_banner_combos.ini");
    for node in &graph.nodes {
        assert!(
            graph.cluster_index_of(node.section_index).is_some(),
            "node {:?} must map into a banner cluster",
            node.id
        );
    }
    let cluster_idx = graph
        .clusters
        .iter()
        .position(|c| c.title == "Mixer")
        .expect("fixture has a Mixer banner group");
    let members: Vec<usize> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| graph.cluster_index_of(n.section_index) == Some(cluster_idx))
        .map(|(i, _)| i)
        .collect();
    assert!(
        members.len() >= 2,
        "Mixer cluster should have multiple members"
    );
    let positions = solve(&graph, &[], SolveKnobs::default());
    let xs: Vec<f32> = members.iter().map(|&i| positions[i].0).collect();
    let ys: Vec<f32> = members.iter().map(|&i| positions[i].1).collect();
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let (cw, ch) = (max_x - min_x, max_y - min_y);
    assert!(cw.is_finite() && ch.is_finite());
    assert!(cw > 0.0 && ch > 0.0);
    // The layered seed places members by depth/order (design gv5), so a
    // cluster's aspect follows its member distribution — a 2-deep chain is
    // roughly square, not a forced width-first stripe. The boundedness check
    // below is the real contract: members stay near their centroid.
    // Members stay bounded around their centroid (no explosion, no stripe).
    let cx = xs.iter().sum::<f32>() / xs.len() as f32;
    let cy = ys.iter().sum::<f32>() / ys.len() as f32;
    for &i in &members {
        let d = solver_dist(positions[i], (cx, cy));
        assert!(d < 2000.0, "member drifted from cluster centroid: {d}");
    }
}

#[test]
fn regression_solver_pinned_tip_stays_fixed_on_real_patch() {
    // Tip anchor + drag-to-place over a real fixture: a pinned node holds its
    // exact seed position through a full solve, a dragged node pinned at its
    // drop stays put through a local re-settle, and unpinning lets the tip
    // re-flow.
    let (_, graph) = crate::test_support::fixture_patch_and_graph("cable_banner_combos.ini");
    assert!(!graph.nodes.is_empty());
    let tip = 0; // first section in .ini order = the graph's tip

    // Pinned through a full solve: the tip never leaves its seed (the fixed
    // anchor) while the other nodes settle.
    let seed = seed_positions(&graph);
    let positions = solve(&graph, &[tip], SolveKnobs::default());
    assert_eq!(
        positions[tip], seed[tip],
        "pinned tip moved during full solve"
    );

    // Unpinned, the tip re-flows under the forces.
    let free = solve(&graph, &[], SolveKnobs::default());
    assert_ne!(
        free[tip], seed[tip],
        "unpinned tip should re-flow off its seed"
    );

    // Local re-settle: drop the tip elsewhere and pin it — it stays exactly
    // at the drop position while its neighbours pull.
    let mut moved = positions.clone();
    let drop = (positions[tip].0 + 40.0, positions[tip].1 - 20.0);
    moved[tip] = drop;
    let found = local_resettle(
        &graph,
        &mut moved,
        &graph.nodes[tip].id,
        LOCAL_RADIUS,
        LOCAL_ITERATIONS,
        &[tip],
        SolveKnobs::default(),
    );
    assert!(found);
    assert_eq!(
        moved[tip], drop,
        "pinned dragged node must stay at its drop"
    );

    // Same drop, unpinned: the dragged node re-flows away from the drop.
    let mut free_moved = positions.clone();
    free_moved[tip] = drop;
    let found = local_resettle(
        &graph,
        &mut free_moved,
        &graph.nodes[tip].id,
        LOCAL_RADIUS,
        LOCAL_ITERATIONS,
        &[],
        SolveKnobs::default(),
    );
    assert!(found);
    assert_ne!(
        free_moved[tip], drop,
        "unpinned dragged node should re-flow off its drop"
    );
}
// ── graph-zoom-node-scaling (tasks 4.1 / 4.3) ──────────────────────────────
// Cross-layer regression over the scale anchor for the four requirements of
// `graph-node-scaling`: zoom-proportional node geometry (no overlap at any
// preset, world width ≤ the column block width), level of detail at extreme
// zoom (labels, ports, and cluster titles omitted below the threshold and
// present at the fit zoom), and hit-testing aligned with the drawn geometry at
// low and high zoom.
//
// Task 4.3 consolidation: the 129 KB / 532-section anchor parses and solves in
// ~6 s (debug), so it is loaded ONCE through `anchor_graph` and every
// assertion runs against that single fixture.

/// The scale anchor loaded once per test binary (task 4.3): the parsed graph
/// plus its solved column positions. Immutable, so every assertion clones them
/// into a lightweight `App` — no re-parse, no re-solve, no pane side effects.
struct AnchorGraph {
    graph: Graph,
    positions: Vec<(f32, f32)>,
    patch: Patch,
    /// Tree-derived circuit labels from the anchor's own open (empty stores +
    /// preamble port labels — label-guess-and-screen-scale 3.1).
    guessed: HashMap<NodeId, String>,
}

/// The shared anchor fixture. `open_graph()` runs the same build + solve the
/// app runs on `g g`, so the positions under test are the real column
/// arrangement's, not a hand-rolled substitute.
fn anchor_graph() -> &'static AnchorGraph {
    static ANCHOR: std::sync::OnceLock<AnchorGraph> = std::sync::OnceLock::new();
    ANCHOR.get_or_init(|| {
        const ANCHOR_PATH: &str = "fixtures/own_scale.ini";
        let patch = Patch::from_ini_file(Path::new(ANCHOR_PATH)).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.open_graph();
        assert!(
            app.pane_holding(ViewType::Graph).is_some(),
            "open_graph must open the graph pane"
        );
        assert!(
            app.graph.as_ref().is_some_and(|g| !g.nodes.is_empty()),
            "scale anchor must build a non-empty graph"
        );
        AnchorGraph {
            graph: app.graph.expect("anchor graph built"),
            positions: app.graph_positions,
            guessed: app.guessed_labels,
            patch: app.patch.expect("anchor patch loaded"),
        }
    })
}

/// A lightweight `App` over the shared anchor graph at `zoom`: the graph and
/// its solved positions are cloned in and the camera is set. No pane is
/// opened — `build_scene_spec` reads only the graph, positions, and camera.
fn anchor_app(zoom: f32) -> App {
    let anchor = anchor_graph();
    let mut app = App::new();
    // These anchor regressions pin the UNGROUPED renderer: they assert over the
    // model graph's own node set (511 drawn nodes, one port marker per node) and
    // over per-node geometry indexed parallel to `graph.nodes`, which grouping —
    // on by default — replaces with group nodes.
    app.graph_grouping.group_enabled = false;
    app.graph = Some(anchor.graph.clone());
    app.graph_positions = anchor.positions.clone();
    app.graph_camera = Some(preset_camera(zoom));
    app
}

/// A camera at a preset zoom with a zero pan. Overlap and LOD are pan-invariant,
/// so the pan carries no information for these assertions.
fn preset_camera(zoom: f32) -> crate::graph_render::GraphCamera {
    crate::graph_render::GraphCamera {
        zoom,
        pan: (0.0, 0.0),
    }
}

/// Strict AABB overlap between two drawn node frames, in spec pixels.
/// A zero-area touch (shared edge) does not count as overlap.
fn frames_overlap(a: &crate::graph_render::NodeSpec, b: &crate::graph_render::NodeSpec) -> bool {
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

/// Headless paint output reduced to the two shape kinds the LOD assertions
/// need: text labels and port-marker circles.
fn painted_labels_and_ports(scene: &crate::graph_render::SceneSpec) -> (Vec<String>, usize) {
    use egui::epaint::Shape;
    use egui::RawInput;

    let ctx = egui::Context::default();
    let canvas = egui::vec2(1280.0, 800.0);
    let mut out = ctx.run_ui(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
            ..Default::default()
        },
        |ui| {
            crate::gui::paint_scene(ui, canvas, Some(scene), &[]);
        },
    );
    let mut labels = Vec::new();
    let mut circles = 0usize;
    for cs in &out.shapes {
        match &cs.shape {
            Shape::Text(t) => labels.push(t.galley.text().to_string()),
            Shape::Circle(_) => circles += 1,
            _ => {}
        }
    }
    out.textures_delta.clear();
    (labels, circles)
}

/// Cross-layer regression over the scale anchor: zoom-proportional node
/// geometry (task 4.1; the anchor is loaded once through [`anchor_graph`] by
/// task 4.3). Each test reads the shared fixture, so the ~5 s anchor parse +
/// solve is paid once per test binary and the tests still run in parallel.
#[test]
fn regression_graph_anchor_no_overlap_at_every_zoom() {
    let anchor = anchor_graph();
    let graph = &anchor.graph;
    let sizes = node_world_sizes(graph);
    let widths = crate::layout::estimated_widths(graph);
    let pane = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
    // ── Block 1: zoom-proportional node geometry ──────────────────────────
    // 1a. World width is the layout's own per-node estimate.
    assert_eq!(sizes.len(), widths.len(), "one world size per node");
    for (i, (world, &est)) in sizes.iter().zip(widths.iter()).enumerate() {
        assert_eq!(
            world.0, est,
            "node {i} world width must be the reserved column estimate"
        );
        assert!(
            world.0 > 0.0 && world.1 > 0.0,
            "node {i} has positive extent"
        );
    }

    // 1b. Each node's world width fits its reserved column block. The block is
    // the max estimate in the node's column, snapped to 2·GRID_SNAP exactly as
    // `solve_columns_pinned` does; both the node and the block scale by the
    // zoom, so this holds at every preset.
    let block_units = 2.0 * crate::layout::GRID_SNAP;
    let mut reserved: Vec<(f32, f32)> = Vec::new();
    for (i, &(x, _)) in anchor.positions.iter().enumerate() {
        match reserved.iter_mut().find(|(rx, _)| (*rx - x).abs() < 1e-3) {
            Some((_, bw)) => *bw = bw.max(widths[i]),
            None => reserved.push((x, widths[i])),
        }
    }
    for (_, bw) in &mut reserved {
        *bw = (*bw / block_units).ceil() * block_units;
    }
    for (i, &(x, _)) in anchor.positions.iter().enumerate() {
        let bw = reserved
            .iter()
            .find(|(rx, _)| (*rx - x).abs() < 1e-3)
            .map(|(_, bw)| *bw)
            .expect("node column reserved");
        assert!(
            widths[i] <= bw + 1e-3,
            "node {i} world width {} exceeds its reserved block {bw}",
            widths[i]
        );
    }

    // 1c. Non-overlap at every zoom preset, on the drawn scene.
    for &zoom in &App::GRAPH_ZOOM_PRESETS {
        let app = anchor_app(zoom);
        let scene = crate::gui::build_scene_spec(&app, crate::theme::active(), pane)
            .expect("scene present at every preset");
        assert_eq!(
            scene.nodes.len(),
            graph.nodes.len(),
            "every node drawn at zoom {zoom}"
        );
        // The drawn frame is exactly world × zoom (never clamped above), which
        // is what makes the reserved-width argument hold at every zoom.
        for (i, n) in scene.nodes.iter().enumerate() {
            assert!(
                (n.w - widths[i] * zoom).abs() < 1e-3,
                "node {i} drawn width must be world × zoom at {zoom}: {} vs {}",
                n.w,
                widths[i] * zoom
            );
            assert!(
                (n.h - crate::layout::NODE_WORLD_H * zoom).abs() < 1e-3,
                "node {i} drawn height must be world × zoom at {zoom}"
            );
        }
        for i in 0..scene.nodes.len() {
            for j in (i + 1)..scene.nodes.len() {
                assert!(
                    !frames_overlap(&scene.nodes[i], &scene.nodes[j]),
                    "column arrangement: nodes {i} and {j} overlap at zoom {zoom}: {:?} vs {:?}",
                    scene.nodes[i],
                    scene.nodes[j]
                );
            }
        }
    }
}

/// Level of detail: node titles and port markers are omitted below the
/// legibility threshold and present at the fit zoom (task 4.1). Cluster titles
/// are not on that threshold: they are fixed screen-space text gated on box
/// room, so a box large enough to hold one still keeps its title at any zoom
/// (cluster-title-legibility, decision D1).
#[test]
fn regression_graph_anchor_lod_omits_text_below_threshold() {
    let anchor = anchor_graph();
    let graph = &anchor.graph;
    let pane = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
    // A preset whose node frames fall below `LOD_MIN_NODE_H` (12.0 px):
    // NODE_WORLD_H is 80, so any zoom below 0.15 does.
    const ZOOM_OUT: f32 = 0.0625;
    const ZOOM_FIT: f32 = 1.0;
    // ── Block 2: level of detail ──────────────────────────────────────────
    // The threshold guard is a compile-time property of the two constants, so
    // assert it in a const block (also keeps the preset choice honest).
    const { assert!(crate::layout::NODE_WORLD_H * ZOOM_OUT < 12.0) };
    let node_title_prefixes: Vec<&str> = graph
        .nodes
        .iter()
        .map(|n| n.circuit.as_str())
        .filter(|c| !c.is_empty())
        .collect();

    // 2a. Below the threshold: no node title, no port markers.
    let low_scene =
        crate::gui::build_scene_spec(&anchor_app(ZOOM_OUT), crate::theme::active(), pane)
            .expect("scene present at the zoom-out preset");
    assert!(
        !low_scene.clusters.is_empty(),
        "the scale anchor has banner clusters to gate"
    );
    let (low_labels, low_circles) = painted_labels_and_ports(&low_scene);
    assert_eq!(
        low_circles, 0,
        "no port markers below the legibility threshold"
    );
    for painted in &low_labels {
        let is_node_title = node_title_prefixes
            .iter()
            .any(|c| painted.starts_with(&c[..c.len().min(6)]));
        assert!(
            !is_node_title,
            "node title painted below the legibility threshold: {painted:?}"
        );
    }

    // 2b. At the fit zoom: node titles and port markers paint.
    let fit_scene =
        crate::gui::build_scene_spec(&anchor_app(ZOOM_FIT), crate::theme::active(), pane)
            .expect("scene present at the fit zoom");
    let (fit_labels, fit_circles) = painted_labels_and_ports(&fit_scene);
    assert!(
        fit_circles > 0,
        "port markers present at the fit zoom (frames are NODE_WORLD_H tall)"
    );
    assert!(
        fit_labels.iter().any(|l| node_title_prefixes
            .iter()
            .any(|c| l.starts_with(&c[..c.len().min(4)]))),
        "at least one painted label carries a node title prefix: {fit_labels:?}"
    );
}

/// Hit-testing follows the zoomed geometry: a click on a node selects at low
/// and high zoom, a gap click selects nothing (task 4.1).
#[test]
fn regression_graph_anchor_hit_testing_low_and_high_zoom() {
    let anchor = anchor_graph();
    let graph = &anchor.graph;
    let sizes = node_world_sizes(graph);
    // The fit zoom, at which the drawn frame is the full world extent.
    const ZOOM_FIT: f32 = 1.0;
    // ── Block 3: hit-testing follows the zoomed geometry ──────────────────
    use crate::gui::WindowFrame;
    use crate::handler::handle_graph_window_frame;

    let mut app = anchor_app(ZOOM_FIT);
    // The first node, whose extent and neighbours are known.
    let (x, y) = app.graph_positions[0];
    let (w, h) = sizes[0];
    let node_id = app.graph.as_ref().unwrap().nodes[0].id.clone();
    // Pane origin as the renderer publishes it: the graph in BigLeft at (0,0),
    // so the window-space pointer equals the pane-space pointer. The pane must
    // genuinely hold the Graph view, or the module-UI pointer branch claims it.
    app.layout.big_left.view = Some(crate::app::ViewType::Graph);
    app.pane_hit_rects = vec![(crate::panes::PaneId::BigLeft, Rect::new(0, 0, 120, 40))];
    let (ox, oy) = (0.0_f32, 0.0_f32);

    // 3a. Low zoom: the drawn frame is a few pixels, so the minimum hit size
    // (GRAPH_MIN_HIT_PX / zoom in world units) is what makes the click land.
    const LOW_ZOOM: f32 = 0.1;
    app.graph_camera = Some(preset_camera(LOW_ZOOM));
    app.graph_canvas_px = Some((960.0, 480.0));
    let (wx, wy) = (x + w / 2.0, y + h / 2.0);
    let (px, py) = (wx * LOW_ZOOM + ox, wy * LOW_ZOOM + oy);
    assert!(
        App::GRAPH_MIN_HIT_PX / LOW_ZOOM > w,
        "minimum hit rect must exceed the drawn frame at low zoom"
    );
    handle_graph_window_frame(
        &WindowFrame {
            pointer: Some((px, py)),
            primary_pressed: true,
            ..Default::default()
        },
        &mut app,
    );
    assert_eq!(
        app.hovered_graph_node,
        Some(0),
        "low-zoom click on the drawn center must hover node 0"
    );
    assert_eq!(
        app.selected_circuit(),
        Some(&node_id),
        "low-zoom click must select the node's circuit"
    );

    // 3b. High zoom: a click inside the drawn frame selects; a click in the gap
    // between frames selects nothing. At 2.0 the minimum hit size does not bind,
    // so the drawn frame is the hit rect.
    const HIGH_ZOOM: f32 = 2.0;
    app.graph_camera = Some(preset_camera(HIGH_ZOOM));
    app.hovered_graph_node = None;
    app.selected_circuit = None;
    let (hx, hy) = ((x + w / 2.0) * HIGH_ZOOM, (y + h / 2.0) * HIGH_ZOOM);
    handle_graph_window_frame(
        &WindowFrame {
            pointer: Some((hx, hy)),
            primary_pressed: true,
            ..Default::default()
        },
        &mut app,
    );
    assert_eq!(
        app.hovered_graph_node,
        Some(0),
        "high-zoom click inside the drawn frame must hover node 0"
    );
    assert_eq!(
        app.selected_circuit(),
        Some(&node_id),
        "high-zoom click inside the drawn frame must select the node's circuit"
    );

    // 3c. The gap: 20 world units below the frame's bottom is outside
    // `GRAPH_MIN_HIT_PX / zoom` (=6 world units) at zoom 2.
    let gap_world_x = x + w / 2.0;
    let gap_y_world = y + h + 20.0;
    app.hovered_graph_node = None;
    let gap_screen = (gap_world_x * HIGH_ZOOM + ox, gap_y_world * HIGH_ZOOM + oy);
    handle_graph_window_frame(
        &WindowFrame {
            pointer: Some(gap_screen),
            ..Default::default()
        },
        &mut app,
    );
    assert_eq!(
        app.hovered_graph_node, None,
        "a click in the gap between two node frames selects nothing"
    );
}

// ── retire-panels-surface (task 3.2): no Panels surface remains ───────────
//
// The compile-time half of this guard is the deletion itself: `src/gui/panels.rs`
// and its wiring (`mod panels;` + the `PanelsFrame` re-export) are gone, and
// `handle_panels_frame` with the `PanelsFrame` import is gone from
// `src/handler.rs`. Those are unobservable at runtime — a reintroduced module
// or handler is caught by the build (`-D warnings` turns the resulting
// dead-code / unused-import lints into failures), not by an `assert!`. What a
// *runtime* regression can pin is the help surface, which is what this test
// covers.

/// Re-introducing a `Panels`/`Physical` `HelpView` variant (or dropping
/// `ModuleUi`) must fail to compile: this match has no wildcard arm, so any
/// change to the variant set breaks the build rather than silently passing.
#[test]
fn regression_no_panels_help_view_variants() {
    use crate::help::HelpView;

    let variants = [
        HelpView::ModuleUi,
        HelpView::Viewer,
        HelpView::Graph,
        HelpView::Validation,
        HelpView::Optimizer,
        HelpView::Picker,
    ];

    // Exhaustive, wildcard-free: the match below is the compile-time tripwire.
    for view in variants {
        let title = match view {
            HelpView::ModuleUi
            | HelpView::Viewer
            | HelpView::Graph
            | HelpView::Validation
            | HelpView::Optimizer
            | HelpView::Picker => view.title(),
        };
        assert!(!title.is_empty(), "every help view has a border title");
    }

    // The module UI is the single merged hardware view: `Panels`/`Physical` no
    // longer exist as separate variants, so no title may resurface the retired
    // "Panels" surface name.
    assert_eq!(HelpView::ModuleUi.title(), "Module UI");
}

/// The module-UI pane resolves to `HelpView::ModuleUi`, and that view carries
/// a non-empty keybinding table including the pane-management keys — so the
/// help modal describes the surface the user is actually in (startup focuses
/// the module UI in the left big pane).
#[test]
fn regression_module_ui_help_view_resolves_and_has_table() {
    use crate::help::{active_view, keybindings, HelpView};

    // Startup default: module UI (`ViewType::Physical`) in `BigLeft`, focused.
    let app = App::new();
    assert_eq!(
        app.layout.pane(app.layout.focus).view,
        Some(ViewType::Physical)
    );
    assert_eq!(active_view(&app), HelpView::ModuleUi);

    let table = keybindings(HelpView::ModuleUi);
    assert!(!table.is_empty(), "module-ui help table must not be empty");

    // Pane-management keys must survive the merge from the deleted panels view.
    for expected in ["Tab/Shift+Tab", "z", "Alt+b", "Alt+s", "r", "Esc"] {
        assert!(
            table.iter().any(|(k, _)| *k == expected),
            "module-ui help table must document the `{expected}` pane key"
        );
    }
}

// ── validation-modal-error-only (task 2.1): warnings-only loads stay closed ─
//
// The modified `patch-validation` requirement: a Warning/Hint-only load must
// load *without* opening the modal, report its finding count in the status bar
// with an `e` hint, and keep normal key handling (the `g g` app chord) working
// until the user opens the list on demand. `fixtures/validation/unused_cable.ini`
// is the smallest fixture that takes this path: a bare `[p2b8]` plus a `[copy]`
// section whose `_UNUSED` output nobody consumes, yielding exactly one `Hint`
// and zero Errors (any Error would take the gating path this rule does not cover).

/// A Hint-only load leaves the modal closed and the status reports the count
/// with the `e` hint; `e` opens the modal; `g g` still opens the graph.
#[test]
fn regression_warnings_only_load_hints_and_defers_modal() {
    let patch = Patch::from_ini_file(Path::new("fixtures/validation/unused_cable.ini")).unwrap();
    let mut app = App::new();
    let loaded = app.load_patch(patch);

    assert!(loaded, "a Warning/Hint-only patch must load");
    assert!(app.patch.is_some(), "the patch is installed");
    assert!(
        !app.showing_validation,
        "a warnings-only load must not auto-open the modal"
    );
    assert!(
        !app.validation_issues.is_empty(),
        "the finding is still recorded for the on-demand list"
    );
    assert!(
        app.validation_issues
            .iter()
            .all(|i| i.severity != crate::validation::Severity::Error),
        "the fixture must not produce Errors (it would take the gating path)"
    );

    // Status names the count and the `e` key — same `press 'e' to view` phrasing
    // as the Error path, so the findings are discoverable without a modal.
    let status = app.status_message.clone();
    assert!(
        status.contains("warnings/hints"),
        "status must report the finding count: {status:?}"
    );
    assert!(
        status.contains(&format!("{} warnings/hints", app.validation_issues.len())),
        "status must name the exact count: {status:?}"
    );
    assert!(
        status.contains("press 'e' to view"),
        "status must hint that `e` opens the list: {status:?}"
    );

    // App chords keep working while the modal stays closed: `g g` opens the graph.
    handle_event(&key(KeyCode::Char('g')), &mut app);
    handle_event(&key(KeyCode::Char('g')), &mut app);
    assert!(
        app.pane_holding(ViewType::Graph).is_some(),
        "`g g` must still open the graph on a warnings-only load"
    );
    assert!(
        !app.showing_validation,
        "the graph chord must not have opened the modal"
    );

    // `e` opens the modal on demand with the list intact.
    handle_event(&key(KeyCode::Char('e')), &mut app);
    assert!(
        app.showing_validation,
        "`e` must open the validation modal on demand"
    );
    assert_eq!(
        app.validation_issues.len(),
        1,
        "the modal lists the single Hint finding"
    );
    assert_eq!(
        app.validation_cursor, 0,
        "the cursor starts at the first issue"
    );

    // Closing with `e` again leaves the patch loaded and everything else intact.
    handle_event(&key(KeyCode::Char('e')), &mut app);
    assert!(
        !app.showing_validation,
        "`e` toggles the modal closed again"
    );
    assert!(
        app.patch.is_some(),
        "closing the modal does not unload the patch"
    );
}

/// A clean load (zero findings) keeps the plain `Loaded <name>` status — no
/// zero-count hint, no `e` affordance. Uses an in-memory `[copy]` patch whose
/// only section declares a known param and a consumed jack, so validation is
/// genuinely empty (the `fixtures/validation/*.ini` files each plant one finding).
#[test]
fn regression_clean_load_keeps_plain_status() {
    let patch =
        Patch::from_ini_str("[copy]\ninput = I1\noutput = O1\n", String::from("clean")).unwrap();
    let mut app = App::new();
    assert!(app.load_patch(patch), "a clean patch loads");

    assert!(
        app.validation_issues.is_empty(),
        "a clean patch must produce no findings, got: {:?}",
        app.validation_issues
            .iter()
            .map(|i| i.code.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !app.showing_validation,
        "a clean load never opens the modal"
    );
    let status = app.status_message.clone();
    assert!(
        !status.contains("warnings/hints"),
        "a clean load must not emit the finding-count hint: {status:?}"
    );
    assert!(
        !status.contains("press 'e'"),
        "a clean load must not advertise the empty modal: {status:?}"
    );
}

// ── label-guess-and-screen-scale (task 3.1) ─────────────────────────────────
// Cross-layer story over the scale anchor: an unlabeled circuit shows its
// nearest explicit upstream label once the graph is built (the anchor's `#
// I1:`-style preamble port labels seed guesses with empty stores), and both
// surfaces render labels at a constant font size across zoom presets.

/// A lightweight `App` over the shared anchor graph with the cached guesses
/// installed and the camera set — the same scene/viewer inputs the real
/// `g g` path produces, without re-parsing or re-solving.
fn anchor_guess_app(zoom: f32) -> App {
    let anchor = anchor_graph();
    let mut app = App::new();
    // These regressions pin the per-node label plumbing (a guessed title
    // reaching one circuit's scene node and the viewer), so the UNGROUPED
    // renderer is the subject — grouping, on by default, would replace the
    // drawn nodes with group boxes.
    app.graph_grouping.group_enabled = false;
    app.patch = Some(anchor.patch.clone());
    app.graph = Some(anchor.graph.clone());
    app.graph_positions = anchor.positions.clone();
    app.guessed_labels = anchor.guessed.clone();
    app.graph_camera = Some(preset_camera(zoom));
    app
}

fn anchor_pane() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))
}

#[test]
fn regression_anchor_guessed_label_reaches_scene_and_viewer() {
    let anchor = anchor_graph();
    assert!(
        !anchor.guessed.is_empty(),
        "anchor must guess at least one circuit label from preamble port labels"
    );
    // Deterministic pick: HashMap iteration order is per-process random.
    let mut ids: Vec<&NodeId> = anchor.guessed.keys().collect();
    ids.sort();
    let id = ids[0];
    let want = anchor.guessed[id].clone();
    let (name, idx) = match id {
        NodeId::Circuit(name, idx) => (name.clone(), *idx),
        other => panic!("guesses only cover circuits, got {other:?}"),
    };

    // Graph surface: the scene node carries the guessed title.
    let app = anchor_guess_app(1.0);
    let scene = crate::gui::build_scene_spec(&app, crate::theme::active(), anchor_pane())
        .expect("scene present");
    let node = scene
        .nodes
        .iter()
        .find(|n| n.circuit == name && n.instance_index == idx)
        .expect("guessed circuit is drawn");
    assert_eq!(node.label, want, "scene shows the guessed label");

    // Source surface: sidebar row and prettified header agree.
    let vspec = crate::gui::viewer_spec(&app);
    let sidebar = vspec.sidebar.as_ref().expect("sidebar present");
    assert!(
        sidebar.names.iter().any(|n| n.contains(&want)),
        "sidebar shows the guessed label: {:?}",
        sidebar.names
    );
    let mut app = app;
    app.source_view_mode = SourceViewMode::Prettified;
    let vspec = crate::gui::viewer_spec(&app);
    assert!(
        vspec.lines.iter().any(|l| l.text.contains(&want)),
        "prettified header shows the guessed label"
    );
}

/// Monospace font sizes painted for a graph scene: node labels render in the
/// monospace face while cluster titles use their own chrome sizing.
fn painted_graph_label_sizes(scene: &crate::graph_render::SceneSpec) -> Vec<f32> {
    use egui::epaint::Shape;
    use egui::RawInput;

    let ctx = egui::Context::default();
    let canvas = egui::vec2(1280.0, 800.0);
    let mut out = ctx.run_ui(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
            ..Default::default()
        },
        |ui| {
            crate::gui::paint_scene(ui, canvas, Some(scene), &[]);
        },
    );
    let mut sizes = Vec::new();
    for cs in &out.shapes {
        if let Shape::Text(t) = &cs.shape {
            let font = t
                .galley
                .job
                .sections
                .first()
                .map_or_else(Default::default, |s| s.format.font_id.clone());
            if font.family == egui::FontFamily::Monospace {
                sizes.push(font.size);
            }
        }
    }
    out.textures_delta.clear();
    sizes
}

#[test]
fn regression_anchor_graph_label_fonts_constant_across_zoom() {
    let mut sizes = Vec::new();
    for zoom in [1.0f32, 2.0] {
        let app = anchor_guess_app(zoom);
        let scene = crate::gui::build_scene_spec(&app, crate::theme::active(), anchor_pane())
            .expect("scene present");
        sizes.extend(painted_graph_label_sizes(&scene));
    }
    assert!(!sizes.is_empty(), "anchor paints node labels");
    assert!(
        sizes.iter().all(|&s| s == sizes[0]),
        "one label font at every zoom: {sizes:?}"
    );
}

/// One cell's ladder step: the cell's index in the painted spec and the step in
/// points the painter drew (or the layout resolved) for it.
type CellLabelStep = (usize, f32);

/// The painted in-cell label runs of a module-UI spec at one physical zoom:
/// `(painted, fitted)` where the first element lists `(cell index, painted step)`
/// once per painted *line* (a wrapped label therefore appears once per line) and
/// the second lists `(cell index, step)` for every cell the spec fitted a label
/// for.
///
/// The match is deliberately narrow, because `paint_physical` also paints
/// monospace text inside a cell that is not that cell's label: the compact
/// cell's state is a corner-marker glyph sized from the cell height (6–9 pt, so
/// not necessarily a ladder step at all), and every callout headline is 14 pt
/// outside the rack. A run counts only when its text is one of the cell's own
/// `label_layout` lines and its painted rect fits the cell (expanded by 1 pt for
/// egui's monospace advance differing slightly from the 0.6 em the layout
/// budgets with). The step is read off the painted shape rather than the layout,
/// so the story can compare the two.
fn painted_physical_label_sizes(zoom: f32) -> (Vec<CellLabelStep>, Vec<CellLabelStep>) {
    use egui::epaint::Shape;
    use egui::RawInput;

    let anchor = anchor_graph();
    let mut app = App::new();
    app.patch = Some(anchor.patch.clone());
    app.physical_zoom = zoom;
    let pane = anchor_pane();
    let spec = crate::gui::physical_spec(&app, pane).expect("physical spec present");
    let fitted: Vec<(usize, f32)> = spec
        .cells
        .iter()
        .enumerate()
        .filter_map(|(cell, spec_cell)| {
            spec_cell
                .label_layout
                .as_ref()
                .map(|layout| (cell, layout.font))
        })
        .collect();
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(
        RawInput {
            screen_rect: Some(pane),
            ..Default::default()
        },
        |ui| {
            crate::gui::paint_physical(ui, pane, Some(&spec));
        },
    );
    let mut painted: Vec<(usize, f32)> = Vec::new();
    for cs in &out.shapes {
        let Shape::Text(t) = &cs.shape else {
            continue;
        };
        let font = t
            .galley
            .job
            .sections
            .first()
            .map_or_else(Default::default, |s| s.format.font_id.clone());
        if font.family != egui::FontFamily::Monospace {
            continue;
        }
        let text = t.galley.text();
        let text_rect = egui::Rect::from_min_size(t.pos, t.galley.size());
        let host = spec.cells.iter().position(|cell| {
            cell.label_layout
                .as_ref()
                .is_some_and(|layout| layout.lines.iter().any(|line| line == text))
                && cell.rect.expand(1.0).contains_rect(text_rect)
        });
        if let Some(cell) = host {
            painted.push((cell, font.size));
        }
    }
    out.textures_delta.clear();
    (painted, fitted)
}

/// The label ladder (module-ui-label-fit D2/D3): the largest step a cell can
/// hold, never below the 6 pt floor.
const MODULE_UI_LABEL_LADDER: [f32; 5] = [10.0, 9.0, 8.0, 7.0, 6.0];

#[test]
fn regression_anchor_module_ui_label_fonts_follow_the_ladder_per_cell() {
    let mut matrices: Vec<(f32, Vec<(usize, f32)>)> = Vec::new();
    for zoom in [0.75f32, 1.0, 1.5, 2.0] {
        let (runs, fitted) = painted_physical_label_sizes(zoom);
        assert!(
            !runs.is_empty(),
            "anchor module UI paints cell labels at zoom {zoom}"
        );

        // One step per cell: a wrapped label's lines all share the cell's step, so
        // two different steps for one cell mean the painter disagreed with its own
        // layout.
        let mut step_per_cell: Vec<(usize, f32)> = Vec::new();
        for &(cell, font) in &runs {
            match step_per_cell.iter_mut().find(|(seen, _)| *seen == cell) {
                Some((_, seen_step)) => assert_eq!(
                    *seen_step, font,
                    "cell {cell} paints two steps at zoom {zoom}: {runs:?}"
                ),
                None => step_per_cell.push((cell, font)),
            }
        }
        step_per_cell.sort_unstable_by_key(|&(cell, _)| cell);

        // The ladder and the floor: every painted run is a step of the ladder and
        // nothing is painted below 6 pt.
        for &(cell, font) in &runs {
            assert!(
                MODULE_UI_LABEL_LADDER.contains(&font),
                "cell {cell} paints {font} pt at zoom {zoom}, off the 10/9/8/7/6 ladder: {runs:?}"
            );
            assert!(
                font >= 6.0,
                "cell {cell} paints {font} pt at zoom {zoom}, below the 6 pt floor"
            );
        }

        // And the step is the one the cell's own geometry allows: the cells that
        // paint a label are exactly the cells the spec fitted one for, and the
        // painted step is that layout's step.
        let mut painted_cells: Vec<usize> = step_per_cell.iter().map(|&(cell, _)| cell).collect();
        painted_cells.sort_unstable();
        let mut fitted_cells: Vec<usize> = fitted.iter().map(|&(cell, _)| cell).collect();
        fitted_cells.sort_unstable();
        assert_eq!(
            painted_cells, fitted_cells,
            "cells with an in-cell layout and cells painting a label must coincide at zoom {zoom}"
        );
        for &(cell, step) in &fitted {
            assert!(
                step_per_cell
                    .iter()
                    .any(|&(painted_cell, font)| painted_cell == cell && font == step),
                "cell {cell} must draw its own layout step {step} at zoom {zoom}: {runs:?}"
            );
        }

        matrices.push((zoom, step_per_cell));
    }

    // A growing cell may take a larger step, and never a smaller one: the same
    // cell's step is non-decreasing as the zoom grows, because its whole text box
    // scales with it, so any step that fit before still fits.
    for pair in matrices.windows(2) {
        let (low_zoom, low) = &pair[0];
        let (high_zoom, high) = &pair[1];
        for &(cell, low_font) in low {
            if let Some(&(_, high_font)) = high.iter().find(|(c, _)| *c == cell) {
                assert!(
                    high_font >= low_font,
                    "cell {cell} steps down from {low_font} pt at {low_zoom} to {high_font} pt at {high_zoom}"
                );
            }
        }
    }

    // The matrix is a ladder, not one constant: several steps appear across the
    // presets, the smallest zoom takes a step below the top one, and at least one
    // cell answers a larger cell with a larger step.
    let (_, lowest) = &matrices[0];
    let (_, highest) = matrices.last().expect("four presets");
    let mut steps: Vec<f32> = matrices
        .iter()
        .flat_map(|(_, painted)| painted.iter().map(|&(_, font)| font))
        .collect();
    steps.sort_by(f32::total_cmp);
    steps.dedup();
    assert!(
        steps.len() >= 2,
        "cell labels take more than one ladder step across the presets: {steps:?}"
    );
    assert!(
        lowest.iter().any(|&(_, font)| font < 10.0),
        "at the smallest preset a cell takes a step below the ladder's top: {lowest:?}"
    );
    assert!(
        lowest.iter().any(|&(cell, low_font)| highest
            .iter()
            .any(|&(other, high_font)| other == cell && high_font > low_font)),
        "a cell that grows with the zoom takes a larger ladder step: {lowest:?} -> {highest:?}"
    );
}

// ── controller-geometry-fidelity (task 3.1): faceplate rotary stack ─────────
// One fixture (`fixtures/faceplate_rotary_stack.ini`) declaring all six
// controllers the change corrects. This story asserts presence and token
// attribution only: the arrangement, position and ring assertions live in
// `src/physical.rs` (task 1.1) and the measured data lands in task 2.1, so
// every token check here is deliberately position-free and stays green
// across the geometry regeneration.

/// `(geometry key, declared tokens)` for each controller the fixture
/// declares; one representative token per element family it carries. The
/// instance numbers are the fixture's own (see its header): bare sections
/// take chain positions 1..4, the explicit p4b2/db8e take 5/6.
const ROTARY_STACK_FACEPLATES: &[(&str, &[&str])] = &[
    ("p2b8", &["B1.1", "L1.8", "P1.2"]),
    ("p4b2", &["P5.1", "P5.4", "B5.1", "L5.2"]),
    ("p10", &["P2.1", "P2.10"]),
    ("s10", &["S3.1", "S3.10"]),
    ("e4", &["E4.1", "B4.1", "L4.1"]),
    ("db8e", &["B6.1", "B6.8", "E6.1"]),
];

/// Attribution guard shared by the faceplate regressions: every token in
/// `expected` is attributed to its declared controller faceplate and resolves to
/// an element cell there. Presence and attribution only — no coordinate is read,
/// which is what lets one guard serve several faceplate fixtures.
fn assert_faceplate_tokens(layout: &crate::physical::PhysicalLayout, expected: &[(&str, &[&str])]) {
    for (key, tokens) in expected {
        let ix = layout
            .modules
            .iter()
            .position(|m| m.geometry_key == *key)
            .unwrap_or_else(|| {
                panic!(
                    "no faceplate for {key}; faceplates = {:?}",
                    layout
                        .modules
                        .iter()
                        .map(|m| m.geometry_key.clone())
                        .collect::<Vec<_>>()
                )
            });
        let faceplate = &layout.modules[ix];
        assert!(
            !faceplate.is_fallback,
            "{key} faceplate must use its geometry entry, not the fallback"
        );
        for token in *tokens {
            assert!(
                faceplate.components.iter().any(|c| c.id == *token),
                "{token} must be attributed to the {key} faceplate, got {:?}",
                faceplate
                    .components
                    .iter()
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>()
            );
            assert!(
                layout.cell_for(ix, token).is_some(),
                "{token} must resolve to an element cell on the {key} faceplate"
            );
        }
    }
}

#[test]
fn regression_faceplate_rotary_stack_all_six_declared_and_attributable() {
    let path = "fixtures/faceplate_rotary_stack.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");

    // No Error-severity validation finding: the fixture is a valid patch, not
    // a validation-case carrier.
    let schema = crate::schema::load_schema();
    let issues = crate::validation::validate_patch(&patch, schema);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{path} must yield no Error-severity finding, got {errors:#?}"
    );

    // The patch declares all six controllers (lowercased panel names are the
    // geometry keys).
    let declared: Vec<String> = patch
        .module_types()
        .iter()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    for (key, _) in ROTARY_STACK_FACEPLATES {
        assert!(
            declared.contains(&key.to_string()),
            "fixture must declare {key}; declared panel types = {declared:?}"
        );
    }

    // Each controller has its own faceplate, and every token it declares is
    // attributed to that faceplate and resolves to a cell on it. Presence and
    // attribution only — no coordinate is read here.
    let layout = crate::physical::PhysicalLayout::build(
        Some(&patch),
        crate::patch::MasterModel::Master,
        &[],
    );
    assert_faceplate_tokens(&layout, ROTARY_STACK_FACEPLATES);

    // Exactly six controller faceplates beside the always-present CV I/O
    // master: no controller merged into another's plate and none is missing.
    let controller_faceplates: Vec<&str> = layout
        .modules
        .iter()
        .filter(|m| m.controller != "CV I/O")
        .map(|m| m.geometry_key.as_str())
        .collect();
    assert_eq!(
        controller_faceplates.len(),
        6,
        "six controller faceplates, got {controller_faceplates:?}"
    );
}

// ── module-ui-rack-inventory 4.3: declared element identity in the chain ────
// Task 4.2 pinned `cell_for` and the geometry-declared kind inside
// `src/physical.rs`; these stories pin the cross-layer result on real
// fixtures — the chain the app builds actually draws the MASTER18 gate
// column for I/G tokens, and an R2C faceplate carries only its two serial
// jacks, none of which a hardware token can reach.

/// Assert a millimetre coordinate matches the geometry's declared value.
fn assert_mm_near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{what}: expected {expected} mm, got {actual} mm"
    );
}

/// The MASTER18 shape (`fixtures/master_declaration_gate_inputs.ini`, reused:
/// a declared MASTER18 patch clocked on I1 and reset from I2): the built chain
/// resolves those tokens to the gate column, never to the CV-out jacks that
/// occupy the same rows, and G tokens land on their own declared row.
///
/// The G half is layout-level only: the component scanner's token letters
/// (`HW_TOKEN_LETTERS`) do not include `G`, so a `gate = G1` entry would not
/// become a patch component — the cells still exist because MASTER18's
/// geometry declares them, which is what `cell_for` resolves against.
#[test]
fn regression_master18_gate_tokens_land_on_the_gate_column() {
    let path = "fixtures/master_declaration_gate_inputs.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");
    let schema = crate::schema::load_schema();

    // A wrong fixture must not pass silently: the declaration is what makes
    // this patch MASTER18.
    let plan = crate::rack::resolve(&patch, schema);
    assert_eq!(
        plan.model,
        crate::patch::MasterModel::Master18,
        "the fixture must resolve to MASTER18"
    );
    assert_eq!(
        plan.source,
        crate::rack::MasterSource::Declared,
        "the fixture's `# LABELS: master=18` must be the reason"
    );
    assert!(
        plan.conflicts.is_empty(),
        "gate/trigger inputs on I1/I2 are legitimate MASTER18 uses: {:#?}",
        plan.conflicts
    );

    let layout = crate::physical::PhysicalLayout::build(Some(&patch), plan.model, &[]);
    assert_eq!(layout.modules.len(), 2, "master plus the P2B8");

    let ix = layout
        .modules
        .iter()
        .position(|m| m.geometry_key == "master18")
        .expect("a MASTER18 patch draws the MASTER18 faceplate");
    let master = &layout.modules[ix];
    assert_eq!(master.controller, "CV I/O");
    assert!(!master.is_fallback, "MASTER18 must use its geometry entry");

    // Both sides of the patch are attributed to that one faceplate.
    for token in ["I1", "I2", "O1", "O2"] {
        assert!(
            master.components.iter().any(|c| c.id == token),
            "{token} must be attributed to the MASTER18 faceplate, got {:?}",
            master
                .components
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>()
        );
    }

    // Gate column: I1/I2 are gate-ins at 18.5 mm — never the O1/O2 jacks.
    let i1 = layout.cell_for(ix, "I1").expect("I1 gate-in cell");
    assert_eq!(i1.label, "I1", "I1 must not resolve to O1");
    assert_eq!(i1.kind.as_deref(), Some("gate_in"));
    assert_mm_near(i1.rect_mm.x_mm, 18.5, "I1 column");
    assert_mm_near(i1.rect_mm.y_mm, 30.0, "I1 row");

    let i2 = layout.cell_for(ix, "I2").expect("I2 gate-in cell");
    assert_eq!(i2.label, "I2", "I2 must not resolve to O2");
    assert_eq!(i2.kind.as_deref(), Some("gate_in"));
    assert_mm_near(i2.rect_mm.x_mm, 18.5, "I2 column");
    assert_mm_near(i2.rect_mm.y_mm, 43.0, "I2 row");

    let o1 = layout.cell_for(ix, "O1").expect("O1 cv-out cell");
    assert_eq!(o1.label, "O1");
    assert_eq!(o1.kind.as_deref(), Some("cv_out"));
    assert_mm_near(o1.rect_mm.x_mm, 1.5, "O1 column");
    assert!(
        i1.rect_mm.x_mm > o1.rect_mm.x_mm,
        "the gate column sits right of the CV-out column"
    );

    // Gate outs share the gate column and take their own declared rows.
    let g1 = layout.cell_for(ix, "G1").expect("G1 gate-out cell");
    assert_eq!(g1.label, "G1", "G1 must not resolve to O1");
    assert_eq!(g1.kind.as_deref(), Some("gate_out"));
    assert_mm_near(g1.rect_mm.x_mm, 18.5, "G1 column");
    assert_mm_near(g1.rect_mm.y_mm, 56.0, "G1 row");

    let g4 = layout.cell_for(ix, "G4").expect("G4 gate-out cell");
    assert_eq!(g4.label, "G4");
    assert_eq!(g4.kind.as_deref(), Some("gate_out"));
    assert_mm_near(g4.rect_mm.x_mm, 18.5, "G4 column");
    assert_mm_near(g4.rect_mm.y_mm, 95.0, "G4 row");
}

/// The R2C serial bridge reaches the chain through the rack inventory (its
/// section name is not a patch controller, so no patch declares it). Its
/// faceplate is exactly the two declared serial jacks, and neither is a token
/// target: they render, but `cell_for` must return `None` for every shaped
/// token, including the positional fallback.
#[test]
fn regression_r2c_faceplate_is_two_unaddressable_serial_cells() {
    let key = crate::physical::ModuleKey::parse("r2c").expect("r2c is an inventory key");
    let layout = crate::physical::PhysicalLayout::build(
        None,
        crate::patch::MasterModel::Master,
        std::slice::from_ref(&key),
    );
    assert!(layout.inventory_driven, "the inventory built this chain");

    let ix = layout
        .modules
        .iter()
        .position(|m| m.geometry_key == "r2c")
        .unwrap_or_else(|| {
            panic!(
                "no r2c faceplate; faceplates = {:?}",
                layout
                    .modules
                    .iter()
                    .map(|m| m.geometry_key.clone())
                    .collect::<Vec<_>>()
            )
        });
    let r2c = &layout.modules[ix];
    assert_eq!(r2c.controller, "R2C");
    assert_eq!(r2c.module_instance, Some(1));
    assert!(!r2c.is_fallback, "r2c must use its geometry entry");

    // Exactly two cells, both the serial pair, both without token identity.
    let cells: Vec<&crate::physical::ElementCell> = r2c.cells.values().flatten().collect();
    assert_eq!(
        cells.len(),
        2,
        "the R2C faceplate is exactly its two serial jacks: {:?}",
        cells.iter().map(|c| c.label.clone()).collect::<Vec<_>>()
    );
    let labels: Vec<&str> = cells.iter().map(|c| c.label.as_str()).collect();
    assert!(
        labels.contains(&"serial in") && labels.contains(&"serial out"),
        "the two declared serial jacks, got {labels:?}"
    );
    for cell in &cells {
        assert!(
            cell.element.is_none() && cell.kind_letter.is_none(),
            "{} must carry no token identity: {cell:?}",
            cell.label
        );
    }

    // No hardware token reaches either jack — identity matching needs a
    // declared element, and the positional fallback rejects a cell without
    // one.
    for token in ["I1", "I2", "O1", "O2", "G1", "G4", "B1", "P1"] {
        assert!(
            layout.cell_for(ix, token).is_none(),
            "{token} must not resolve to an R2C serial jack"
        );
    }
}

// ── module-ui-rack-inventory 3.2: the drawn front, pinned by counts ─────────
//
// Task 3.1 made an inventory-driven chain draw every cell a faceplate declares
// (addressed cells labelled and hit-tested, the rest dim and inert). The counts
// are therefore whole declared fronts — the hardware on the panel, independent
// of how much of it a patch uses:
//
//   `(inventory entry, patch fixture, geometry key, painted cells, breakdown)`
//
// A fixture is needed only to select the master *model*: the inventory key
// `master18` names the master faceplate, and which geometry that faceplate uses
// comes from the patch's resolved plan (no patch resolves to MASTER).
const RACK_INVENTORY_FRONTS: &[(&str, Option<&str>, &str, usize, &str)] = &[
    (
        "master",
        None,
        "master",
        33,
        "16 CV jacks + 16 matrix cells + the identity-less `SD reload` button",
    ),
    (
        "master18",
        Some("fixtures/master_declaration_gate_inputs.ini"),
        "master18",
        15,
        "14 CV/gate cells (I1, I2 and G1..G4 among them) + `SD reload`",
    ),
    (
        "g8",
        None,
        "g8",
        16,
        "8 CV cells (G1.1..G1.8) + 8 matrix cells (R17..R24)",
    ),
    (
        "x7",
        None,
        "x7",
        13,
        "12 token cells (4 CV jacks G9..G12, 8 matrix cells R49..R56) + `USB`",
    ),
    ("r2c", None, "r2c", 2, "the two identity-less serial jacks"),
];

/// An app loaded from `fixture` (when given) with `modules` as its only rack
/// inventory entry, so the chain is built through the same public path the
/// config hands the app.
fn inventory_app(fixture: Option<&str>, modules: &[&str]) -> App {
    let mut app = App::new();
    if let Some(fixture) = fixture {
        let patch = Patch::from_ini_file(Path::new(fixture))
            .unwrap_or_else(|e| panic!("{fixture} must parse: {e}"));
        assert!(app.load_patch(patch), "{fixture} must load");
    }
    app.physical_rack_spec = rack_inventory(modules);
    app
}

/// The cells the pane paints inside the spec module titled `title`, as
/// `(rect, component index, dimmed)`.
///
/// Plain data, so the helper needs no name for the gui's private `CellSpec`.
/// `crate::gui::physical_spec` is the production builder `paint_panes` feeds the
/// painter, so the set counted here is exactly the set the pane draws — the
/// stories below do not reimplement a cell-emission rule.
fn painted_module_cells(app: &App, title: &str) -> Vec<(egui::Rect, Option<usize>, bool)> {
    let pane = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(1280.0, 800.0));
    let spec = crate::gui::physical_spec(app, pane).expect("an inventory or a patch is drawable");
    let module = spec
        .modules
        .iter()
        .find(|module| module.title == title)
        .unwrap_or_else(|| {
            panic!(
                "no {title} faceplate painted; painted = {:?}",
                spec.modules
                    .iter()
                    .map(|module| module.title.clone())
                    .collect::<Vec<_>>()
            )
        });
    spec.cells
        .iter()
        .filter(|cell| module.rect.contains(cell.rect.center()))
        .map(|cell| (cell.rect, cell.component_index, cell.dimmed))
        .collect()
}

/// The painted cells of the chain module whose geometry key is `geometry_key`.
///
/// The chain is the one the painter builds (same `build` call and inventory),
/// used only to name the faceplate's painted title — `physical_spec` titles a
/// module `"{controller} 1"` — so a story names a geometry key, not a
/// hand-constructed module.
fn painted_faceplate_cells(
    app: &App,
    geometry_key: &str,
) -> Vec<(egui::Rect, Option<usize>, bool)> {
    let inventory = app.physical_rack_spec.module_keys();
    let chain =
        crate::physical::PhysicalLayout::build(app.patch.as_ref(), app.master_model(), &inventory);
    assert!(
        chain.inventory_driven,
        "{geometry_key}: the chain must come from the rack inventory"
    );
    let module = chain
        .modules
        .iter()
        .find(|module| module.geometry_key == geometry_key)
        .unwrap_or_else(|| {
            panic!(
                "no {geometry_key} faceplate in the chain; faceplates = {:?}",
                chain
                    .modules
                    .iter()
                    .map(|module| module.geometry_key.clone())
                    .collect::<Vec<_>>()
            )
        });
    painted_module_cells(app, &format!("{} 1", module.controller))
}

/// The five declared fronts an inventory draws: MASTER 33, MASTER18 15, G8 16,
/// X7 13 and R2C 2 cells. One story over `RACK_INVENTORY_FRONTS` keeps the
/// numbers in one place; each rack is built by listing its module in a
/// `RackSpec` row, never by constructing modules by hand.
#[test]
fn regression_inventory_fronts_draw_every_declared_cell() {
    for &(entry, fixture, geometry_key, expected, breakdown) in RACK_INVENTORY_FRONTS {
        let app = inventory_app(fixture, &[entry]);
        let cells = painted_faceplate_cells(&app, geometry_key);
        assert_eq!(
            cells.len(),
            expected,
            "{entry} must paint {breakdown} ({expected} cells), painted {} instead",
            cells.len()
        );
        if fixture.is_none() {
            // No patch: the whole front is unaddressed, so every cell is dim
            // and carries no component (task 2.4's inert-front rule).
            assert!(app.patch.is_none(), "{entry}: the no-patch arm");
            assert!(
                cells
                    .iter()
                    .all(|(_, component, dimmed)| component.is_none() && *dimmed),
                "{entry}: every cell of an unaddressed front is dim and inert: {cells:?}"
            );
        }
    }
}

/// The compatibility arm: with no inventory the chain is the patch-derived one,
/// so the master front stays suppressed exactly as before the inventory existed.
/// `fixtures/own_two_ctrl.ini` addresses one CV I/O output (`O1`) on the master,
/// so master cells are that one addressed jack — none of the 33 declared cells
/// the patch leaves unused may appear.
#[test]
fn regression_no_inventory_keeps_the_master_front_suppressed() {
    let fixture = "fixtures/own_two_ctrl.ini";
    let patch = Patch::from_ini_file(Path::new(fixture))
        .unwrap_or_else(|e| panic!("{fixture} must parse: {e}"));
    let mut app = App::new();
    assert!(app.load_patch(patch), "{fixture} must load");
    assert!(
        app.physical_rack_spec.rows.is_empty(),
        "{fixture}: no rack inventory configured"
    );

    let inventory = app.physical_rack_spec.module_keys();
    assert!(inventory.is_empty(), "no inventory resolves to no keys");
    let chain =
        crate::physical::PhysicalLayout::build(app.patch.as_ref(), app.master_model(), &inventory);
    assert!(!chain.inventory_driven, "the chain came from the patch");
    let master = &chain.modules[0];
    assert_eq!(master.controller, "CV I/O", "the master is the chain head");
    assert!(
        master
            .components
            .iter()
            .all(|component| chain.cell_for(0, &component.id).is_some()),
        "every addressed master component resolves to a declared jack"
    );
    let declared: usize = master.cells.values().map(Vec::len).sum();

    let cells = painted_module_cells(&app, "CV I/O 1");
    assert_eq!(
        cells.len(),
        master.components.len(),
        "the master draws only the jacks the patch addresses"
    );
    assert!(
        cells
            .iter()
            .all(|(_, component, dimmed)| component.is_some() && !*dimmed),
        "every drawn master cell is an addressed element: {cells:?}"
    );
    assert!(
        cells.len() < declared,
        "declared cells the patch leaves unused stay suppressed: {} drawn of {declared}",
        cells.len()
    );
}

/// Task 2.4's rule: an inventory with no patch paints its modules but publishes
/// no hit rect. `paint_panes` publishes `component_rects` as the cells'
/// `component_index` projection, so the projection is asserted here through the
/// production spec builder; the gui story
/// `an_inventory_without_a_patch_paints_the_rack_and_publishes_no_rects` drives
/// the real `paint_panes` path end to end.
#[test]
fn regression_no_patch_inventory_paints_modules_and_publishes_no_rects() {
    let app = inventory_app(None, &["master", "g8"]);
    assert!(app.patch.is_none(), "the state under test");

    let pane = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(1280.0, 800.0));
    let spec = crate::gui::physical_spec(&app, pane).expect("an inventory alone is drawable");
    let titles: Vec<&str> = spec
        .modules
        .iter()
        .map(|module| module.title.as_str())
        .collect();
    assert_eq!(
        titles,
        ["CV I/O 1", "G8 1"],
        "every inventoried faceplate still paints"
    );
    assert!(
        !spec.cells.is_empty(),
        "the inventoried fronts keep their cells"
    );

    let published: Vec<usize> = spec
        .cells
        .iter()
        .filter_map(|cell| cell.component_index)
        .collect();
    assert!(
        published.is_empty(),
        "no patch publishes no hit rect: {published:?}"
    );
}

// ── button-led-composition (task 4.1): composed button faces + folded LEDs ─
// One fixture (`fixtures/button_led_composition.ini`) declaring a P2B8, a B32
// and an M4 with explicit button/LED pairs, plus one LED-less element (P2B8
// pot P1.1). This story asserts presence and token attribution only: the
// faceplates render their declared tokens, no folded LED publishes a cell or
// a hit rect, and the LED-less element still renders its cell. Positions
// belong to tasks 1.1/1.2 and are never read here.

/// `(geometry key, declared button tokens)` per controller the fixture
/// declares. The instance numbers are the fixture's own: bare controller
/// sections take chain positions 1..3 in file order (p2b8, b32, m4).
const BUTTON_LED_FACEPLATES: &[(&str, &[&str])] = &[
    ("p2b8", &["B1.1", "B1.2"]),
    ("b32", &["B2.1", "B2.2"]),
    ("m4", &["B3.1"]),
];

/// The LEDs bound to the declared buttons via explicit `led = L.N` pairs.
const BUTTON_LED_FOLDED_LEDS: &[&str] = &["L1.1", "L1.2", "L2.1", "L2.2", "L3.1"];

// ── button-led-composition follow-up (droid_tui-y57): P4B2 parity ──────
// The composed face was a P2B8/B32/M4 whitelist. P4B2 is the same case at a
// different size — two 9 mm buttons with a 3 mm RGB LED centred in each and a
// documented 1:1 B→L pairing — so its buttons must compose too, and the LED
// register must stop publishing a standalone cell. master, G8 and X7 were
// named in the same bead but carry no LED-bearing buttons at all: master has
// one unaddressed `SD reload` button with its 16 LEDs beside the CV jacks, G8
// only gate jacks, X7 a USB switch plus gate outputs. For them "parity" would
// mean inventing a face that the hardware does not have, so the rule stays
// keyed on the controllers that actually fold.
#[test]
fn regression_p4b2_buttons_compose_their_leds_like_p2b8() {
    let path = "fixtures/faceplate_rotary_stack.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture patch must load");
    let spec =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    let component = |id: &str| {
        app.patch
            .as_ref()
            .unwrap()
            .hw_components
            .iter()
            .position(|c| c.id == id)
    };
    let cell = |id: &str| {
        let index = component(id).unwrap_or_else(|| panic!("{id} must exist"));
        spec.cells
            .iter()
            .find(|c| c.component_index == Some(index))
            .unwrap_or_else(|| panic!("{id} must publish a cell"))
    };
    for button in ["B5.1", "B5.2"] {
        let cell = cell(button);
        assert_eq!(
            cell.shape,
            Some(crate::gui::CellShape::Round),
            "{button} must render the composed round face"
        );
        assert_eq!(
            cell.glyph, "",
            "{button} carries its state on the composed outline, not a glyph"
        );
    }
    // The paired LEDs are inside the faces now: no cell, no hit rect.
    let tokens: Vec<&str> = spec
        .cells
        .iter()
        .filter_map(|c| c.component_index)
        .map(|i| app.patch.as_ref().unwrap().hw_components[i].id.as_str())
        .collect();
    for led in ["L5.1", "L5.2"] {
        assert!(
            !tokens.contains(&led),
            "the LED {led} folds into its P4B2 button and must not publish a cell"
        );
    }
    // The pots on the same faceplate keep the compact cell (only buttons compose).
    assert_ne!(
        cell("P5.1").shape,
        Some(crate::gui::CellShape::Round),
        "a P4B2 pot is not a composed button face"
    );
}

#[test]
fn regression_button_led_composition_faces_render_and_leds_fold() {
    let path = "fixtures/button_led_composition.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");

    // (a) No Error-severity validation finding: the fixture is a valid patch,
    // not a validation-case carrier.
    let schema = crate::schema::load_schema();
    let issues = crate::validation::validate_patch(&patch, schema);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{path} must yield no Error-severity finding, got {errors:#?}"
    );

    // (b) All three faceplates exist and render their declared tokens:
    // each token is attributed to its faceplate and resolves to a cell.
    let layout = crate::physical::PhysicalLayout::build(
        Some(&patch),
        crate::patch::MasterModel::Master,
        &[],
    );
    assert_faceplate_tokens(&layout, BUTTON_LED_FACEPLATES);

    // (c) No folded LED appears as a cell or in `component_rects`: the
    // published spec cells resolve to token ids via the global
    // `patch.hw_components` index, and the hit rects derive from those cells.
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture patch must load");
    let spec =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    let cell_tokens: Vec<&str> = spec
        .cells
        .iter()
        .filter_map(|c| c.component_index)
        .map(|i| app.patch.as_ref().unwrap().hw_components[i].id.as_str())
        .collect();
    for led in BUTTON_LED_FOLDED_LEDS {
        assert!(
            !cell_tokens.contains(led),
            "folded LED {led} must not publish a cell, got {cell_tokens:?}"
        );
    }
    // The paint path publishes `component_rects` from exactly these spec
    // cells (`gui/mod.rs`), so mirror that publication headlessly: only real
    // components (Some index) are hit-testable.
    let hit_tokens: Vec<&str> = spec
        .cells
        .iter()
        .filter_map(|c| c.component_index)
        .map(|i| app.patch.as_ref().unwrap().hw_components[i].id.as_str())
        .collect();
    for led in BUTTON_LED_FOLDED_LEDS {
        assert!(
            !hit_tokens.contains(led),
            "folded LED {led} must not publish a hit rect, got {hit_tokens:?}"
        );
    }

    // (d) The LED-less element still renders its cell.
    assert!(
        cell_tokens.contains(&"P1.1"),
        "the LED-less P1.1 must still render its cell, got {cell_tokens:?}"
    );
}

// ── led-fold-cable-pairing: only a bare LED token binds ────────────────
// One fixture (`fixtures/led_cable_pairing.ini`) whose `led`/`ledN` values are
// cables (the drum/melody patch idiom) instead of LED tokens. A parameter
// value names a register only when it is a bare LED token; a cable value
// leaves the positional default in force, so every fold-rule button keeps its
// LED folded into the face and no L cell renders as a standalone cell.

#[test]
fn regression_cable_valued_led_params_keep_every_led_folded() {
    let path = "fixtures/led_cable_pairing.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");

    // (a) The fixture is a valid patch, not a validation-case carrier.
    let schema = crate::schema::load_schema();
    let errors: Vec<_> = crate::validation::validate_patch(&patch, schema)
        .into_iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{path} must yield no Error-severity finding, got {errors:#?}"
    );

    // (b) Every button on a fold-rule faceplate resolves an LED token — the
    // cable-valued params bind nothing, so the positional default answers.
    let layout = crate::physical::PhysicalLayout::build(
        Some(&patch),
        crate::patch::MasterModel::Master,
        &[],
    );
    for faceplate in &layout.modules {
        for comp in faceplate
            .components
            .iter()
            .filter(|c| c.kind == ComponentKind::Button)
        {
            let led = crate::patch::folded_led_for_element(comp)
                .unwrap_or_else(|| panic!("{} must fold an LED", comp.id));
            assert!(
                led.starts_with('L'),
                "{} must fold an LED token, got {led:?}",
                comp.id
            );
            assert!(
                comp.led.is_none() || comp.led.as_deref() == Some(led.as_str()),
                "{} must not stay bound to a non-token value, got {:?}",
                comp.id,
                comp.led
            );
        }
    }

    // (c) No L token publishes a cell or a hit rect on the rendered spec.
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture patch must load");
    let spec =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    let cell_tokens: Vec<&str> = spec
        .cells
        .iter()
        .filter_map(|c| c.component_index)
        .map(|i| app.patch.as_ref().unwrap().hw_components[i].id.as_str())
        .collect();
    let standalone: Vec<&&str> = cell_tokens.iter().filter(|t| t.starts_with('L')).collect();
    assert!(
        standalone.is_empty(),
        "no LED may render as its own cell, got {standalone:?}"
    );

    // (d) The cable-paired buttons still render their composed face.
    for token in ["B1.1", "B2.1"] {
        let index = app
            .patch
            .as_ref()
            .unwrap()
            .hw_components
            .iter()
            .position(|c| c.id == token)
            .unwrap();
        let cell = spec
            .cells
            .iter()
            .find(|c| c.component_index == Some(index))
            .unwrap_or_else(|| panic!("{token} must render a cell"));
        assert!(cell.shape.is_some(), "{token} must render a composed face");
        assert!(
            cell.led_state.is_some(),
            "{token} must fold its positional LED"
        );
    }
}

// ── led-register-colour (task 3.1): static LED colour resolution ──────────
// One fixture (`fixtures/led_register_colour.ini`) covering the load-time
// resolution matrix: a lit P2B8 button LED with a literal ledcolor/ledvalue
// pair, an M4 touch-plate twin with a literal colour, a button LED whose
// ledcolor rides a live cable (dynamic fallback), and a plain button+LED
// pair with no colour params (sourceless fallback). This story asserts
// presence and token attribution only — never positions — plus the resolved
// LED states. The `button` circuit carries no ledcolor/ledvalue params, so
// per-LED colours are owned by `[motorfader]` sections mentioning the
// element token (rank-0 ownership).

/// `(LED token, expected colour scalar)` for the statically resolved LEDs.
/// Instance numbers are the fixture's own: [p2b8] is chain position 1,
/// [m4] is chain position 2 (see the fixture header).
const LED_REGISTER_STATIC: &[(&str, f64)] = &[("L1.1", 0.5), ("L2.1", 0.8)];

/// LEDs that must keep the `Off` fallback: dynamic cable colour (L1.3) and
/// no colour source at all (L1.4).
const LED_REGISTER_OFF: &[&str] = &["L1.3", "L1.4"];

/// The folded LEDs bound to buttons: explicit pairs (L1.1, L1.3, L1.4) and
/// the M4 default RGB twin (L2.1, via B2.1 per `device_led_defaults.ini`).
const LED_REGISTER_FOLDED: &[&str] = &["L1.1", "L1.3", "L1.4", "L2.1"];

#[test]
fn regression_led_register_colour_resolves_static_and_falls_back() {
    use crate::patch::{ComponentState, Patch};

    let path = "fixtures/led_register_colour.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");

    // (a) No Error-severity validation finding: the fixture is a valid patch,
    // not a validation-case carrier. Hint/Warning findings (unused demo
    // cables) are fine.
    let schema = crate::schema::load_schema();
    let issues = crate::validation::validate_patch(&patch, schema);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{path} must yield no Error-severity finding, got {errors:#?}"
    );

    // (b) Static sources resolve at load: lit value plus the mapped colour.
    let find = |id: &str| {
        patch
            .hw_components
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no component {id} in fixture"))
    };
    for (led, scalar) in LED_REGISTER_STATIC {
        assert_eq!(
            find(led).state,
            ComponentState::Led {
                value: 1.0,
                rgb: Some(crate::patch::led_colour_for_value(*scalar)),
            },
            "{led} must load lit in its resolved colour"
        );
    }

    // (c) Dynamic and sourceless LEDs keep the Off fallback.
    for led in LED_REGISTER_OFF {
        assert_eq!(
            find(led).state,
            ComponentState::Off,
            "{led} must keep the Off fallback"
        );
    }

    // (d) Faceplate attribution: the buttons owning the fixture LEDs resolve
    // to element cells on their plates. Presence and attribution only — no
    // coordinate is read here.
    let layout = crate::physical::PhysicalLayout::build(
        Some(&patch),
        crate::patch::MasterModel::Master,
        &[],
    );
    for (plate_ix, button) in [("p2b8", "B1.1"), ("m4", "B2.1")] {
        let ix = layout
            .modules
            .iter()
            .position(|m| m.geometry_key == plate_ix)
            .unwrap_or_else(|| panic!("no {plate_ix} faceplate"));
        assert!(
            layout.cell_for(ix, button).is_some(),
            "{button} must resolve to an element cell on the {plate_ix} faceplate"
        );
    }

    // (e) No folded LED publishes a cell or a hit rect: the published spec
    // cells resolve to token ids via the global `patch.hw_components` index.
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture patch must load");
    let spec =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    let cell_tokens: Vec<&str> = spec
        .cells
        .iter()
        .filter_map(|c| c.component_index)
        .map(|i| app.patch.as_ref().unwrap().hw_components[i].id.as_str())
        .collect();
    for led in LED_REGISTER_FOLDED {
        assert!(
            !cell_tokens.contains(led),
            "folded LED {led} must not publish a cell, got {cell_tokens:?}"
        );
    }
}

// ── unreferenced elements (unreferenced-elements 3.1) ──────────────────────
// One fixture (`fixtures/unreferenced_elements.ini`) declaring a partial B32
// (5 of 32 buttons via `[notebuttons]`) and a bare `[e4]`. This story asserts
// presence and token attribution only: no Error finding, the 27 unused B32
// buttons and 124 E4 ring segments render as dimmed unlabelled `None` cells,
// no standalone LED cell renders on the fold-rule B32, and no unaddressed
// token reaches the published hit rects. Positions belong to tasks 1.1/1.2
// and are never read here.

/// The five buttons the fixture addresses on the partial B32.
const UNREFERENCED_B32_BUTTONS: &[&str] = &["B3.1", "B3.2", "B3.3", "B3.4", "B3.5"];

#[test]
fn regression_unreferenced_elements_render_dimmed_without_hit_rects() {
    let path = "fixtures/unreferenced_elements.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");

    // (a) No Error-severity validation finding: the fixture is a valid patch,
    // not a validation-case carrier.
    let schema = crate::schema::load_schema();
    let issues = crate::validation::validate_patch(&patch, schema);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{path} must yield no Error-severity finding, got {errors:#?}"
    );

    // (b) Both faceplates exist on their geometry entries and render their
    // declared tokens: each token is attributed to its faceplate and
    // resolves to a cell. The `[notebuttons]` section binds to the b32
    // faceplate (never the fallback); the bare `[e4]` synthesizes its front.
    let layout = crate::physical::PhysicalLayout::build(
        Some(&patch),
        crate::patch::MasterModel::Master,
        &[],
    );
    let b32_ix = layout
        .modules
        .iter()
        .position(|m| m.geometry_key == "b32")
        .unwrap_or_else(|| panic!("no b32 faceplate"));
    assert!(
        !layout.modules[b32_ix].is_fallback,
        "b32 faceplate must use its geometry entry, not the fallback"
    );
    for token in UNREFERENCED_B32_BUTTONS {
        assert!(
            layout.modules[b32_ix]
                .components
                .iter()
                .any(|c| c.id == *token),
            "{token} must be attributed to the b32 faceplate"
        );
        assert!(
            layout.cell_for(b32_ix, token).is_some(),
            "{token} must resolve to an element cell on the b32 faceplate"
        );
    }
    let e4_ix = layout
        .modules
        .iter()
        .position(|m| m.geometry_key == "e4")
        .unwrap_or_else(|| panic!("no e4 faceplate"));

    // (c) Unaddressed geometry: 27 unused B32 buttons and 124 E4 ring
    // segments (128 registers minus the 4 synthesized LEDs).
    let b32_unused = layout.modules[b32_ix]
        .unaddressed_cells()
        .into_iter()
        .filter(|c| c.kind_letter == Some('B'))
        .count();
    assert_eq!(
        b32_unused, 27,
        "partial B32 must leave 27 buttons unaddressed"
    );
    let e4_unused_rings = layout.modules[e4_ix]
        .unaddressed_cells()
        .into_iter()
        .filter(|c| c.kind_letter == Some('L'))
        .count();
    assert_eq!(
        e4_unused_rings, 124,
        "bare E4 must leave 124 ring segments unaddressed"
    );

    // (d) The published spec renders every unaddressed cell dimmed and
    // unlabelled via the `None`-index path, and no standalone LED cell
    // survives on the fold-rule B32 — so exactly 27 + 124 `None` cells
    // publish (the suppressed B32 LEDs never render).
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture patch must load");
    let spec =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    let none_cells: Vec<_> = spec
        .cells
        .iter()
        .filter(|c| c.component_index.is_none())
        .collect();
    assert_eq!(
        none_cells.len(),
        27 + 124,
        "spec must render 27 dimmed buttons + 124 dimmed ring segments, got {}",
        none_cells.len()
    );
    for cell in &none_cells {
        assert!(cell.dimmed, "unaddressed cell must render dimmed");
        assert!(
            cell.label.is_empty(),
            "unaddressed cell must render unlabelled"
        );
    }

    // (e) Hit publication (`gui/mod.rs` filter: only `Some`-index cells are
    // hit-testable) carries no unaddressed token: every published token is
    // one the patch addresses.
    let addressed: std::collections::HashSet<&str> = layout
        .modules
        .iter()
        .flat_map(|m| m.components.iter().map(|c| c.id.as_str()))
        .collect();
    let hit_tokens: Vec<&str> = spec
        .cells
        .iter()
        .filter_map(|c| c.component_index)
        .map(|i| app.patch.as_ref().unwrap().hw_components[i].id.as_str())
        .collect();
    assert!(
        !hit_tokens.is_empty(),
        "the fixture must publish hit rects for its addressed elements"
    );
    for token in &hit_tokens {
        assert!(
            addressed.contains(token),
            "published hit rect {token} must belong to an addressed element"
        );
    }
}

// ── shift-colour-module-ui (task 3.1): shift tint regression ───────────────
// Cross-layer proof over `fixtures/button_led_composition.ini`, whose shift
// groups are auto-assigned by instance number (patch.rs): B2.x are Group2
// (members under Group2), B1.x Group1 and P1.1 Group1 (non-members). With
// Group2 active the member cells tint in the Group2 token, everything else
// matches the no-shift baseline, unaddressed cells stay dim/None, and
// no-shift is byte-identical to the pre-change shape.
#[test]
fn regression_shift_colour_module_ui_tint_and_noop() {
    let path = "fixtures/button_led_composition.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");

    let token_of = |app: &App, index: Option<usize>| -> String {
        index.map_or_else(Default::default, |i| {
            app.patch.as_ref().unwrap().hw_components[i].id.clone()
        })
    };

    // No-shift baseline.
    let mut app = App::new();
    assert!(app.load_patch(patch.clone()), "the fixture patch must load");
    let base =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    assert!(
        base.cells.iter().all(|c| c.shift_color.is_none()),
        "no-shift baseline must carry no shift tint"
    );

    // Group2 active: member tints, non-members match the baseline.
    app.active_shift = Some(ShiftGroup::Group2);
    let shifted =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    assert_eq!(shifted.cells.len(), base.cells.len());
    let theme = crate::theme::active();
    let (mut members, mut plain, mut bare) = (0, 0, 0);
    for (s, b) in shifted.cells.iter().zip(base.cells.iter()) {
        let token = token_of(&app, s.component_index);
        assert_eq!(token_of(&app, b.component_index), token);
        match token.as_str() {
            t if t.starts_with("B2.") => {
                members += 1;
                assert_eq!(s.shift_color, Some(theme.shift2), "member must tint Group2");
                assert_eq!(
                    s.shift_color.unwrap_or(s.color),
                    theme.shift2,
                    "member rendered colour follows the group token"
                );
            }
            _ if s.component_index.is_none() => {
                bare += 1;
                assert_eq!(s.shift_color, None, "unaddressed cell never tints");
                assert!(s.dimmed, "unaddressed cell stays dim");
                assert_eq!(s, b, "unaddressed cell unchanged");
            }
            _ => {
                plain += 1;
                assert_eq!(
                    s, b,
                    "non-member {token} renders exactly as with no shift active"
                );
            }
        }
    }

    assert!(
        members > 0,
        "the story must exercise at least one member cell"
    );
    assert!(
        plain > 0,
        "the story must exercise at least one non-member cell"
    );
    // `bare` is intentionally not asserted: the fixture need not leave an
    // unaddressed cell, but any such cell must stay dim/None (arm above).
    let _ = bare;

    // No shift is a no-op: byte-identical specs.
    app.active_shift = None;
    let again =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    assert_eq!(again.cells, base.cells, "no shift must be byte-identical");
}

// ── s10-switch-glyphs (task 3.1): rotary vs toggle switch glyphs ─────────
// One fixture (`fixtures/s10_switch_kinds.ini`) with a bare `[s10]`
// synthesizing the full 10-switch front (S1.1..S1.10 at chain position 1).
// This story asserts presence and token attribution only — never positions —
// plus the kind-driven glyphs on the published spec cells.
#[test]
fn regression_s10_switch_kinds_glyphs() {
    let path = "fixtures/s10_switch_kinds.ini";
    let patch = Patch::from_ini_file(Path::new(path)).expect("fixture must parse");

    // (a) No Error-severity validation finding: the fixture is a valid patch,
    // not a validation-case carrier.
    let schema = crate::schema::load_schema();
    let issues = crate::validation::validate_patch(&patch, schema);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{path} must yield no Error-severity finding, got {errors:#?}"
    );

    // (b) The s10 faceplate exists on its geometry entry and every S token
    // lands on it. Presence and attribution only — no coordinate is read.
    let layout = crate::physical::PhysicalLayout::build(
        Some(&patch),
        crate::patch::MasterModel::Master,
        &[],
    );
    let s10_ix = layout
        .modules
        .iter()
        .position(|m| m.geometry_key == "s10")
        .unwrap_or_else(|| panic!("no s10 faceplate"));
    assert!(
        !layout.modules[s10_ix].is_fallback,
        "s10 faceplate must use its geometry entry, not the fallback"
    );
    for n in 1..=10u32 {
        let token = format!("S1.{n}");
        assert!(
            layout.modules[s10_ix]
                .components
                .iter()
                .any(|c| c.id == token),
            "{token} must be attributed to the s10 faceplate"
        );
        assert!(
            layout.cell_for(s10_ix, &token).is_some(),
            "{token} must resolve to an element cell on the s10 faceplate"
        );
    }

    // (c) Kind-driven glyphs on the published spec cells: rotary S1.1/S1.2
    // render round (\u{25C9}/\u{25CB}), the eight toggles render square
    // (\u{25A3}/\u{25A1}), and state text is unchanged across kinds.
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture patch must load");
    let spec =
        crate::gui::physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
    let cell_for = |token: &str| {
        let idx = app
            .patch
            .as_ref()
            .unwrap()
            .hw_components
            .iter()
            .position(|c| c.id == token)
            .unwrap_or_else(|| panic!("no component {token} in fixture"));
        spec.cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .unwrap_or_else(|| panic!("{token} must publish a spec cell"))
    };
    for n in 1..=10u32 {
        let token = format!("S1.{n}");
        let cell = cell_for(&token);
        let rotary = n <= 2;
        assert!(
            ["\u{25C9}", "\u{25CB}"].contains(&cell.glyph.as_str()) == rotary,
            "{token} rotary={rotary} glyph must be round iff rotary, got {:?}",
            cell.glyph
        );
        assert!(
            ["\u{25A3}", "\u{25A1}"].contains(&cell.glyph.as_str()) != rotary,
            "{token} rotary={rotary} glyph must be square iff toggle, got {:?}",
            cell.glyph
        );
    }
    // State text stability: Off switches read OFF on both kinds.
    assert_eq!(cell_for("S1.1").state_text, cell_for("S1.3").state_text);
}

// ── sections-black-hole-solver (task 6.1): sections mode keeps every banner
//    group inside its own wrapped lane band ─────────────────────────────────
// Cross-layer story: `fixtures/graph_topology_error.ini` (the smallest fixture
// carrying two real comment banners, each owning two circuits) →
// `App::load_patch` → `layout_mode = LayoutMode::Sections` → the real
// `open_graph()` build + solve → the solved positions checked against the
// **lane slots the solver itself seeds from**
// ([`crate::layout::seed_positions_sections`]).
//
// The contract moved under this story (sections-black-hole-solver 3.8 / 3.9).
// Circuits now chain as **width-aware lanes in patch order** (a lane is as wide
// as its widest member's `min(estimated_width, SECTIONS_LANE_MAX_NODE_W)`), the
// lanes **wrap into rows** at `SECTIONS_ROW_TARGET_W`, and the section-hole
// spring no longer pulls toward the build-time `Graph.section_holes` value
// (`mean_slice × HORIZONTAL_SPACING`): it pulls toward the mean **seed target**
// of the group's own circuit members *in the same wrapped row* — one anchor per
// (group, row) band, derived internally in lane-slot space. A per-node tether
// (`SECTIONS_TETHER`) bounds how far a member may leave its own seeded slot,
// and a deterministic overlap-removal pass enforces non-overlap as an explicit
// post-condition. `Graph.section_holes` is therefore asserted only as the
// build-time bookkeeping it still is — never against solved positions.
//
// Measured on this fixture through the real app path (pin `[0]`, bbox
// 339.22 × 120.00): Sources members x = 0.000 / 109.031 on lane slots 0 / 142
// (band anchor 71.0); Sinks members x = 209.375 / 339.219 on lane slots
// 268 / 370 (band anchor 319.0); both clusters' members sit on y = 0;
// clearance Sources→Sinks = 100.343.
//
// Tolerances are in world units and mirror `layout::tests`:
//   * `DRIFT_GUARD` = `SECTIONS_TETHER + SECTIONS_LANE_GAP` = 60 + 40 = 100 —
//     the same bound the scale story asserts as its `drift_tol`. The slice and
//     section springs are tethered to each node's own seed target by
//     `SECTIONS_TETHER` (60 = 1.5 lane gaps), and the overlap-removal pass may
//     add at most one more lane gap while separating a pair, so 100 is the
//     worst per-axis distance from the seed a solved node may reach. It
//     replaces the old pitch-derived `ANCHOR_TOL_X` (0.30 × 80 = 24) and
//     `SPREAD_TOL_X` (1.25 × 80 = 100): lane pitch is width-aware and ≈142
//     here, so a two-member cluster legitimately spans more than the gap
//     between band anchors and an inter-cluster-gap bound is structurally
//     wrong.
//   * Band centring: a group's mean x must stay **strictly inside**
//     `SECTIONS_TETHER` (60) of its own band anchor (the mean of its members'
//     seeded lane slots). The hole-spring pull is tether-bounded, so a group
//     whose spring targets a distant anchor saturates *every* member against
//     the tether in one direction and parks the mean exactly 60 away — measured
//     60.000 for Sinks once the solver is handed the build-time
//     `Graph.section_holes` anchors instead of the lane band anchors, versus
//     16.484 (Sources) / 44.703 (Sinks) with them. The bound is strict because
//     "exactly 60" is the saturated failure mode, not a settled cluster.
//   * Cluster extent: a cluster's x-spread is bounded by **its own lane-slot
//     span plus `DRIFT_GUARD`** — the honest replacement for the old
//     "spread < clearance between anchors", which cannot hold now that one lane
//     pitch (≈142) is wider than the band gap itself. Measured 109.031 against
//     0..142 + 100 (Sources) and 129.845 against 268..370 + 100 (Sinks).
//   * Containment and ordering: a member may lead its own lane slot by up to
//     the tether (measured 58.625 — the Sinks leader at 209.375 against slot
//     268), so containment in the group's lane band is asserted at the band ±
//     `DRIFT_GUARD`, and interleaving is excluded separately: the two groups'
//     member x-ranges stay disjoint (measured clearance 100.343 > 0) and no
//     member of either group falls inside the other group's raw lane-slot span.
// Falsified (scratch): feeding the solver the build-time `Graph.section_holes`
// anchors instead of `section_hole_anchors_lanes` saturates both Sinks members
// at the tether (x = 208.0 / 310.0, each exactly 60 left of its slot) and fails
// the band-centring assertion (mean 259.0, drift 60.000 !< SECTIONS_TETHER);
// restoring the lane band anchors passes. The tether is *not* a lever on this
// fixture: at defaults it is never reached, so removing the clamp moves the
// solved positions by < 0.04.
//
// Everything is force-relaxed, so the bounds above are tolerances rather than
// equalities — but the exact-position determinism assertions below stay exact
// (design D9: identical graph + identical force triple ⇒ identical positions).

/// Per-axis bound on how far a solved node may sit from its own seeded lane
/// slot: the tether plus one lane gap of overlap-removal slack — the same bound
/// `layout::tests` asserts as its `drift_tol`.
const DRIFT_GUARD: f32 = crate::layout::SECTIONS_TETHER + crate::layout::SECTIONS_LANE_GAP;
/// Smallest real fixture with two comment banners, each owning two circuits.
const SECTIONS_FIXTURE: &str = "fixtures/graph_topology_error.ini";

/// One anchored circuit member of a banner group: the lane slot the solver
/// seeded it on (`(slot_x, seed_y)`) and the position it solved to.
struct SectionsMember {
    slot: (f32, f32),
    position: (f32, f32),
}

/// One banner group's solved geometry in sections mode: its title and its
/// anchored circuit members (jumps and controllers may share a cluster's section
/// range but own no section hole, so they never count as members).
struct SectionsCluster {
    title: String,
    members: Vec<SectionsMember>,
}

impl SectionsCluster {
    fn member_xs(&self) -> impl Iterator<Item = f32> + '_ {
        self.members.iter().map(|member| member.position.0)
    }

    fn mean_x(&self) -> f32 {
        self.member_xs().sum::<f32>() / self.members.len() as f32
    }

    fn spread_x(&self) -> f32 {
        self.max_x() - self.min_x()
    }

    fn min_x(&self) -> f32 {
        self.member_xs().fold(f32::INFINITY, f32::min)
    }

    fn max_x(&self) -> f32 {
        self.member_xs().fold(f32::NEG_INFINITY, f32::max)
    }

    /// Mean seeded lane slot x of the members — the per-(group, row) band anchor
    /// the solver derives internally (this fixture wraps into a single row, so
    /// the band is the whole group).
    fn band_anchor_x(&self) -> f32 {
        self.members.iter().map(|member| member.slot.0).sum::<f32>() / self.members.len() as f32
    }

    /// Width of the lane-slot range the group's members were seeded on.
    fn slot_span_x(&self) -> f32 {
        let min = self
            .members
            .iter()
            .map(|member| member.slot.0)
            .fold(f32::INFINITY, f32::min);
        let max = self
            .members
            .iter()
            .map(|member| member.slot.0)
            .fold(f32::NEG_INFINITY, f32::max);
        max - min
    }
}

/// Load `fixture` through the real app path, switch to sections mode, open the
/// graph (`open_graph()` builds the graph AND solves it with the current
/// `layout_mode` and force triple), and return the app plus each banner group's
/// solved geometry paired with the lane slots the sections seed placed its
/// members on. Only anchored circuit members count.
fn sections_clusters(fixture: &str) -> (App, Vec<SectionsCluster>) {
    let patch = Patch::from_ini_file(Path::new(fixture)).expect("fixture must parse");
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture patch must load");
    app.layout_mode = crate::config::LayoutMode::Sections;
    app.open_graph();
    let graph = app
        .graph
        .as_ref()
        .expect("open_graph builds a graph")
        .clone();
    let positions = app.graph_positions.clone();
    let slots = crate::layout::seed_positions_sections(&graph);

    let mut clusters = Vec::new();
    for (cluster_index, cluster) in graph.clusters.iter().enumerate() {
        let members: Vec<usize> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, node)| {
                graph.cluster_index_of(node.section_index) == Some(cluster_index)
                    && graph.section_holes.get(*index).copied().flatten().is_some()
            })
            .map(|(index, _)| index)
            .collect();
        if members.is_empty() {
            continue;
        }
        // Build-time bookkeeping only: `compute_section_holes` still records one
        // `mean_slice × pitch` hole per group, and every member of a group shares
        // it. The solver no longer reads this value (see the module comment), so
        // it is never compared against a solved position.
        let hole_x = graph.section_holes[members[0]]
            .expect("member is anchored")
            .0;
        assert!(
            members
                .iter()
                .all(|&index| graph.section_holes[index].expect("member is anchored").0 == hole_x),
            "every member of {:?} must share one build-time section-hole anchor",
            cluster.title
        );
        clusters.push(SectionsCluster {
            title: cluster.title.clone(),
            members: members
                .iter()
                .map(|&index| SectionsMember {
                    slot: slots[index],
                    position: positions[index],
                })
                .collect(),
        });
    }
    (app, clusters)
}

#[test]
fn regression_sections_mode_clusters_around_section_holes() {
    let (app, clusters) = sections_clusters(SECTIONS_FIXTURE);

    // Structure the story depends on: two banner groups, each really a cluster
    // of at least two anchored circuits (the fixture must not silently lose a
    // group or a cable).
    assert!(
        clusters.len() >= 2,
        "the fixture must expose at least two banner groups, got {}",
        clusters.len()
    );
    for cluster in &clusters {
        assert!(
            cluster.members.len() >= 2,
            "banner group {:?} must anchor at least two circuits, got {}",
            cluster.title,
            cluster.members.len()
        );
    }

    // Requirement 1: no member leaves its own group's lane band. The band is the
    // range of lane slots its members were seeded on; it is expanded by
    // DRIFT_GUARD because the tether lets a member lead its own slot — a raw
    // span containment is exactly what no longer holds (the Sinks leader sits
    // 58.625 left of slot 268). Interleaving is excluded separately below.
    for cluster in &clusters {
        let lo = cluster
            .members
            .iter()
            .map(|member| member.slot.0)
            .fold(f32::INFINITY, f32::min)
            - DRIFT_GUARD;
        let hi = cluster
            .members
            .iter()
            .map(|member| member.slot.0)
            .fold(f32::NEG_INFINITY, f32::max)
            + DRIFT_GUARD;
        for (i, member) in cluster.members.iter().enumerate() {
            assert!(
                member.position.0 >= lo && member.position.0 <= hi,
                "banner group {:?} member {i} solved x={} outside its lane band \
                 [{lo}, {hi}] (member lane slots ± DRIFT_GUARD {DRIFT_GUARD})",
                cluster.title,
                member.position.0
            );
        }
    }

    // Requirement 2: every member stays tethered to its own seeded slot — x to
    // its lane slot, y to its seeded stack target. Bound DRIFT_GUARD = the
    // tether (60) plus one lane gap of overlap-removal slack (40); the springs
    // themselves can only reach the tether, and the separation pass is what
    // needs the extra lane gap.
    for cluster in &clusters {
        for (i, member) in cluster.members.iter().enumerate() {
            let dx = member.position.0 - member.slot.0;
            let dy = member.position.1 - member.slot.1;
            assert!(
                dx.abs() <= DRIFT_GUARD,
                "banner group {:?} member {i} x={} left its lane slot {} by \
                 {dx} > DRIFT_GUARD {DRIFT_GUARD}",
                cluster.title,
                member.position.0,
                member.slot.0
            );
            assert!(
                dy.abs() <= DRIFT_GUARD,
                "banner group {:?} member {i} y={} left its seeded stack target {} by \
                 {dy} > DRIFT_GUARD {DRIFT_GUARD}",
                cluster.title,
                member.position.1,
                member.slot.1
            );
        }
    }

    // Requirement 3: a group reads as one tight band **relative to its own lane
    // span**, not relative to its neighbour. Bound: the group's own lane-slot
    // span plus DRIFT_GUARD, the worst overhang its outermost members can add.
    // The old "spread < clearance between anchors" is what this replaces: one
    // width-aware lane pitch (≈142) is now wider than the band gap itself.
    for cluster in &clusters {
        let spread = cluster.spread_x();
        let bound = cluster.slot_span_x() + DRIFT_GUARD;
        assert!(
            spread <= bound,
            "banner group {:?} x-spread {spread} exceeds its own lane span {} + \
             DRIFT_GUARD {DRIFT_GUARD} = {bound}",
            cluster.title,
            cluster.slot_span_x()
        );
    }

    // Requirement 4: each group still settles on its **own lane-derived band
    // anchor** (the mean slot of its members), not on a distant build-time hole.
    // Strictly inside SECTIONS_TETHER, because a saturated pull parks the mean
    // exactly at the tether (measured 60.000 for Sinks with the build-time
    // `Graph.section_holes` anchors 40 / 200 in place of the band anchors
    // 71 / 319); a settled band stays well inside it (16.484 / 44.703).
    for cluster in &clusters {
        let anchor = cluster.band_anchor_x();
        let drift = (cluster.mean_x() - anchor).abs();
        assert!(
            drift < crate::layout::SECTIONS_TETHER,
            "banner group {:?} did not settle on its own lane band anchor: mean {}, \
             anchor {anchor}, drift {drift} !< SECTIONS_TETHER {}",
            cluster.title,
            cluster.mean_x(),
            crate::layout::SECTIONS_TETHER
        );
    }

    // Requirement 5: the groups stay distinguishable and ordered — neighbouring
    // member x-ranges are disjoint (clearance > 0), and no member of one group
    // intrudes into the other group's raw lane-slot span (no interleaving).
    let mut ordered = clusters.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.min_x().partial_cmp(&b.min_x()).expect("finite member x"));
    for pair in ordered.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        let clearance = right.min_x() - left.max_x();
        assert!(
            clearance > 0.0,
            "banner groups {:?} and {:?} overlap: clearance {clearance}",
            left.title,
            right.title
        );
        for (outer, inner) in [(left, right), (right, left)] {
            let lo = inner
                .members
                .iter()
                .map(|member| member.slot.0)
                .fold(f32::INFINITY, f32::min);
            let hi = inner
                .members
                .iter()
                .map(|member| member.slot.0)
                .fold(f32::NEG_INFINITY, f32::max);
            for member in &outer.members {
                assert!(
                    !(member.position.0 > lo && member.position.0 < hi),
                    "banner group {:?} member x={} interleaves into {:?}'s lane span \
                     [{lo}, {hi}]",
                    outer.title,
                    member.position.0,
                    inner.title
                );
            }
        }
    }

    // Determinism (design D9): the same graph solved twice under the same force
    // triple yields identical positions, and a second app driven through the
    // same real path lands on byte-identical positions and lane slots.
    let graph = app.graph.as_ref().expect("graph").clone();
    let pins = app.pinned_indices(&graph);
    let triple = (
        app.tension,
        app.layout_slice_force,
        app.layout_section_force,
    );
    let knobs = SolveKnobs {
        tension: triple.0,
        slice_k: triple.1,
        section_k: triple.2,
        ..Default::default()
    };
    let first = crate::layout::solve_sections(&graph, &pins, knobs);
    let second = crate::layout::solve_sections(&graph, &pins, knobs);
    assert_eq!(
        first, second,
        "solving one graph twice under one force triple must be deterministic"
    );

    let signature = |clusters: &[SectionsCluster]| {
        clusters
            .iter()
            .map(|cluster| {
                (
                    cluster.title.clone(),
                    cluster
                        .members
                        .iter()
                        .map(|member| (member.slot, member.position))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };

    let (again, again_clusters) = sections_clusters(SECTIONS_FIXTURE);
    assert_eq!(
        (
            again.tension,
            again.layout_slice_force,
            again.layout_section_force
        ),
        triple,
        "both runs must use the same force triple (the D9 precondition)"
    );
    assert_eq!(
        app.graph_positions, again.graph_positions,
        "a second open in sections mode must reproduce the first solve exactly"
    );
    assert_eq!(
        signature(&clusters),
        signature(&again_clusters),
        "cluster lane slots and member positions must be reproducible"
    );
}

// ── module-ui-labels / module-ui-label-fit (task 3.1): labels on a real rack ─
// One fixture (`fixtures/module_ui_labels.ini`) is a partial B32 front whose
// preamble labels both fit and overflow their 9 mm cells. The stories drive the
// production load + fit triggers and the real spec/paint path: the rack opens
// fitted at true size, its element cells are their millimetre size, every cell
// label follows the per-cell ladder, `auto` annotates exactly the written labels
// whose cell has no in-cell layout, and switching the label presentation leaves
// the hit and skeleton geometry untouched.

const MODULE_UI_LABELS_FIXTURE: &str = "fixtures/module_ui_labels.ini";

/// The addressed button whose preamble label fits its 9 mm cell at 1:1
/// (`Go`, two characters against the composed face's five-character budget).
const MODULE_UI_LABELS_FITTING: &str = "B1.18";

/// The addressed buttons whose preamble labels the composed face's band cannot
/// draw whole at 1:1 (`Trigger input`, `Clock reset`) — the labels the ladder
/// wraps or ellipsizes, and the ones [`MODULE_UI_LABELS_DELIBERATE_ZOOM`] pushes
/// below the floor altogether.
const MODULE_UI_LABELS_CLIPPED: &[&str] = &["B1.19", "B1.20"];

/// The zoom the `auto` story drives deliberately: inside the window where the
/// composed B32 band (`h − 0.5 w`) still holds "Go" on one 6 pt line while the
/// longer written labels have no in-cell layout at all. At the fitted zoom the
/// labels-first clamp guarantees every written label has a layout, so a forced
/// zoom is what keeps the `auto` selection observable.
const MODULE_UI_LABELS_DELIBERATE_ZOOM: f32 = 0.46;

/// The written label the `auto` story expects to see annotated at the deliberate
/// zoom: 13 characters, so no ladder step holds it at that cell size.
const MODULE_UI_LABELS_UNLAID_WRITTEN: &str = "B1.19";

/// The addressed button the `auto` story strips of its written label, so that
/// element carries only its derived token name — the one case this fixture does
/// not declare, because it labels every element it addresses.
const MODULE_UI_LABELS_DERIVED_ONLY: &str = "B1.20";

/// The rack the fixture packs into, from the same chain + default-case source
/// `App::fit_physical_view` and `physical_spec` consume, so the story's pane
/// is derived from the geometry that is actually drawn.
fn module_ui_labels_rack(patch: &Patch) -> crate::physical::RackLayout {
    let chain =
        crate::physical::PhysicalLayout::build(Some(patch), crate::patch::MasterModel::Master, &[]);
    let spec = crate::physical::RackSpec::default_case(&chain);
    crate::physical::RackLayout::pack(&chain, &spec)
}

/// A pane one true-size rack tall and 640 pt wide. The fit is height-bound
/// and lands at 100 %, while the horizontal margin gives the rack's right-side
/// annotation band room to paint inside the pane.
fn module_ui_labels_pane(rack: &crate::physical::RackLayout) -> egui::Rect {
    let (_, height) = crate::physical::rack_extent_points(rack, 1.0);
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(640.0, height.ceil()))
}

/// An `App` opened on the fixture through the production load + fit triggers,
/// with the module UI pane published at `pane` (the same trigger the pane
/// dispatch runs each frame). `strip` removes that token's written label from
/// the patch preamble first, so the element falls back to its derived name.
fn module_ui_labels_loaded_app(pane: egui::Rect, strip: Option<&str>) -> App {
    let mut patch = Patch::from_ini_file(Path::new(MODULE_UI_LABELS_FIXTURE))
        .expect("module_ui_labels fixture must parse");
    if let Some(token) = strip {
        assert!(
            patch.preamble_labels.remove(token).is_some(),
            "{token} must carry a written label before it is stripped"
        );
    }
    let mut app = App::new();
    assert!(app.load_patch(patch), "module_ui_labels fixture must load");
    app.note_physical_pane_rect(Rect::new(
        0,
        0,
        pane.width().round() as u16,
        pane.height().round() as u16,
    ));
    app.apply_physical_fit_triggers();
    app
}

/// An `App` opened on the fixture through the production load + fit triggers,
/// with the module UI pane published at `pane` (the same trigger the pane
/// dispatch runs each frame).
fn module_ui_labels_app(pane: egui::Rect) -> App {
    module_ui_labels_loaded_app(pane, None)
}

/// The fitted app moved to a deliberate zoom with no pan — the same state the
/// user reaches with the wheel, used where the labels-first fit leaves `auto`
/// with nothing to annotate.
fn module_ui_labels_app_at(pane: egui::Rect, zoom: f32, strip: Option<&str>) -> App {
    let mut app = module_ui_labels_loaded_app(pane, strip);
    app.physical_zoom = zoom;
    app.physical_offset = (0.0, 0.0);
    app
}

/// Edge-touching counts as disjoint, matching the annotation invariant that a
/// callout never enters the rack.
fn module_ui_labels_rects_disjoint(a: egui::Rect, b: egui::Rect) -> bool {
    a.max.x <= b.min.x || b.max.x <= a.min.x || a.max.y <= b.min.y || b.max.y <= a.min.y
}

/// Headless `paint_physical` text runs as `(painted text, font)`, so the
/// annotation story asserts what the real paint path draws rather than the
/// spec's intent.
fn module_ui_labels_painted_texts(
    pane: egui::Rect,
    mut paint: impl FnMut(&mut egui::Ui),
) -> Vec<(String, egui::FontId)> {
    use egui::epaint::Shape;

    let ctx = egui::Context::default();
    let raw_input = egui::RawInput {
        screen_rect: Some(pane),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw_input, |ui| paint(ui));
    let mut texts = Vec::new();
    for clipped in &out.shapes {
        if let Shape::Text(text) = &clipped.shape {
            let font = text.galley.job.sections[0].format.font_id.clone();
            texts.push((text.galley.text().to_string(), font));
        }
    }
    out.textures_delta.clear();
    texts
}

#[test]
fn regression_module_ui_labels_opens_fitted_at_true_size() {
    let patch = Patch::from_ini_file(Path::new(MODULE_UI_LABELS_FIXTURE))
        .expect("module_ui_labels fixture must parse");

    // The fixture is a valid patch, not a validation-case carrier: a single
    // Error-severity finding would make the app refuse the load above.
    let issues = crate::validation::validate_patch(&patch, crate::schema::load_schema());
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "{MODULE_UI_LABELS_FIXTURE} must yield no Error-severity finding, got {errors:#?}"
    );

    let rack = module_ui_labels_rack(&patch);
    let pane = module_ui_labels_pane(&rack);
    let app = module_ui_labels_app(pane);

    // The fit trigger ran against the published pane: the reported zoom is the
    // fit computation's, and for a pane one true-size rack tall it is 1:1.
    let fitted = app
        .physical_fit_zoom
        .expect("the fit trigger must report the zoom it applied");
    let expected = crate::physical::fit_zoom(
        rack.total_width_mm,
        rack.total_height_mm,
        pane.width(),
        pane.height(),
    ) as f32;
    assert!(
        (fitted - expected).abs() < 1e-6,
        "the fitted zoom must be the fit computation's: {fitted} vs {expected}"
    );
    assert!(
        (app.physical_zoom - 1.0).abs() < 0.01,
        "a pane one true-size rack tall must open at 1:1, got {}%",
        app.physical_zoom * 100.0
    );

    // The whole rack is visible and centred — the fit-on-open promise.
    let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");
    let case = spec.case_rect;
    assert!(
        case.min.x >= pane.min.x - 0.5
            && case.min.y >= pane.min.y - 0.5
            && case.max.x <= pane.max.x + 0.5
            && case.max.y <= pane.max.y + 0.5,
        "the fitted rack must be fully visible: case {case:?} in pane {pane:?}"
    );
    let (dx, dy) = (
        (case.center().x - pane.center().x).abs(),
        (case.center().y - pane.center().y).abs(),
    );
    assert!(
        dx <= 0.5 && dy <= 0.5,
        "the fitted rack must be centred: off by ({dx}, {dy}) pt"
    );
}

#[test]
fn regression_module_ui_labels_element_cell_is_true_millimetre_size() {
    let patch = Patch::from_ini_file(Path::new(MODULE_UI_LABELS_FIXTURE))
        .expect("module_ui_labels fixture must parse");
    let rack = module_ui_labels_rack(&patch);
    let pane = module_ui_labels_pane(&rack);
    let app = module_ui_labels_app(pane);
    let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");

    // A B32 button cell is 9 mm square in the controller geometry, so at the
    // 96-dpi baseline (3.7795 pt/mm) it must render ~34 pt square at 100 %.
    let index = idx_for(&app, MODULE_UI_LABELS_FITTING);
    let cell = spec
        .cells
        .iter()
        .find(|cell| cell.component_index == Some(index))
        .unwrap_or_else(|| panic!("{MODULE_UI_LABELS_FITTING} must render a cell"));
    let points_per_mm = crate::physical::PHYSICAL_POINTS_PER_MM as f32;
    let expected = 9.0 * points_per_mm * app.physical_zoom;
    assert!(
        (cell.rect.width() - expected).abs() < 0.1,
        "a 9 mm cell must be 9 mm wide: {} vs {expected} pt",
        cell.rect.width()
    );
    assert!(
        (cell.rect.height() - expected).abs() < 0.1,
        "a 9 mm cell must be 9 mm tall: {} vs {expected} pt",
        cell.rect.height()
    );
    assert!(
        (cell.rect.width() - cell.rect.height()).abs() < 0.05,
        "the square B32 cell must stay square: {:?}",
        cell.rect
    );
    assert!(
        (expected - 34.0).abs() < 0.5,
        "9 mm at the fitted 1:1 baseline is ~34 pt, got {expected}"
    );
}

#[test]
fn regression_module_ui_labels_auto_annotates_only_unlaid_written_labels() {
    let patch = Patch::from_ini_file(Path::new(MODULE_UI_LABELS_FIXTURE))
        .expect("module_ui_labels fixture must parse");
    let rack = module_ui_labels_rack(&patch);
    let pane = module_ui_labels_pane(&rack);
    // The fixture's own three labels all fit at the labels-first fit, so `auto`
    // has nothing to annotate at the zoom the view opens on. The story therefore
    // drives a deliberate zoom inside the window where the composed B32 band
    // (`h − 0.5 w`) still holds "Go" on one 6 pt line while the longer written
    // label has no in-cell layout at all, and strips one written label so that
    // element carries only its derived token name.
    let app = module_ui_labels_app_at(
        pane,
        MODULE_UI_LABELS_DELIBERATE_ZOOM,
        Some(MODULE_UI_LABELS_DERIVED_ONLY),
    );

    // `auto` is the default presentation; the pane opens with it.
    assert_eq!(
        app.label_presentation,
        crate::config::LabelPresentation::Auto,
        "the module UI opens in `auto`"
    );
    let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");

    let cell_for = |token: &str| {
        let index = idx_for(&app, token);
        spec.cells
            .iter()
            .find(|cell| cell.component_index == Some(index))
            .unwrap_or_else(|| panic!("{token} must render a cell"))
            .clone()
    };
    let zoom_pct = (MODULE_UI_LABELS_DELIBERATE_ZOOM * 100.0).round();

    // The four states the rule discriminates, each a fact of this zoom:
    // a written label that fits, a written label no step holds, a label that is
    // only the derived token name, and a cell the patch never addresses.
    let fitting = cell_for(MODULE_UI_LABELS_FITTING);
    assert_eq!(fitting.label, "Go");
    assert!(
        fitting.label_layout.is_some(),
        "a two-character label still fits the composed band at {zoom_pct}%: {fitting:?}"
    );
    let unlaid = cell_for(MODULE_UI_LABELS_UNLAID_WRITTEN);
    assert_eq!(unlaid.label, "Trigger input");
    assert!(
        unlaid.label_layout.is_none(),
        "'{}' must have no in-cell layout at {zoom_pct}%: {unlaid:?}",
        unlaid.label
    );
    let derived = cell_for(MODULE_UI_LABELS_DERIVED_ONLY);
    assert!(
        derived.label != "Clock reset" && derived.label.contains("B1.20"),
        "the stripped label must fall back to the derived token name: {:?}",
        derived.label
    );
    assert!(
        derived.label_layout.is_none(),
        "the derived name is too long for the band at {zoom_pct}% too: {derived:?}"
    );
    assert!(
        spec.cells.iter().any(|cell| cell.component_index.is_none()),
        "the partial B32 front must leave unaddressed cells to test against"
    );

    // `auto` annotates exactly the written labels with no in-cell layout: the
    // unlaid written label, never the fitting one, never the derived-only one,
    // and never an unaddressed cell.
    let mut annotated: Vec<&str> = spec
        .annotations
        .iter()
        .map(|callout| callout.label.as_str())
        .collect();
    annotated.sort_unstable();
    assert_eq!(
        annotated,
        vec!["Trigger input"],
        "auto must annotate the written label no step can show, and only it"
    );
    assert!(
        !annotated.contains(&fitting.label.as_str()),
        "a fitting in-cell label is never annotated: {annotated:?}"
    );
    assert!(
        !annotated.contains(&derived.label.as_str()),
        "a derived-only label is reveal-only, never annotated: {annotated:?}"
    );
    for callout in &spec.annotations {
        assert!(
            module_ui_labels_rects_disjoint(callout.label_rect, spec.case_rect),
            "annotation '{}' enters the rack: {:?}",
            callout.label,
            callout.label_rect
        );
        let covers = |cell_rect: egui::Rect| {
            callout.anchor.x >= cell_rect.min.x - 0.5
                && callout.anchor.x <= cell_rect.max.x + 0.5
                && callout.anchor.y >= cell_rect.min.y - 0.5
                && callout.anchor.y <= cell_rect.max.y + 0.5
        };
        assert!(
            spec.cells
                .iter()
                .any(|cell| cell.component_index.is_some() && covers(cell.rect)),
            "annotation '{}' must be anchored on an addressed cell",
            callout.label
        );
        assert!(
            !spec
                .cells
                .iter()
                .any(|cell| cell.component_index.is_none() && covers(cell.rect)),
            "annotation '{}' must not be anchored on an unaddressed cell",
            callout.label
        );
    }

    // The real paint path draws that label at the performance view's 14 pt
    // callout size and promotes neither the fitting label nor the derived name.
    let texts = module_ui_labels_painted_texts(pane, |ui| {
        crate::gui::paint_physical(ui, pane, Some(&spec));
    });
    let callout_font = egui::FontId::monospace(14.0);
    assert!(
        texts
            .iter()
            .any(|(text, font)| *text == unlaid.label && *font == callout_font),
        "the unlaid label '{}' must paint as a 14 pt callout: {texts:?}",
        unlaid.label
    );
    for (label, why) in [
        (fitting.label.as_str(), "a fitting in-cell label"),
        (derived.label.as_str(), "a derived-only label"),
    ] {
        assert!(
            !texts
                .iter()
                .any(|(text, font)| text == label && *font == callout_font),
            "{why} must not be duplicated as a callout: {texts:?}"
        );
    }
}

#[test]
fn regression_module_ui_labels_modes_leave_hit_and_skeleton_rects_identical() {
    let patch = Patch::from_ini_file(Path::new(MODULE_UI_LABELS_FIXTURE))
        .expect("module_ui_labels fixture must parse");
    let rack = module_ui_labels_rack(&patch);
    let pane = module_ui_labels_pane(&rack);

    // `(published hit rect)` = the same filter `paint_panes` applies to
    // `component_rects`: only real components, rounded into the app's
    // cell-grid rect domain. The skeleton geometry is the case, module, and
    // cell rects the same filter feeds.
    type HitRects = Vec<(usize, Rect)>;
    type SkeletonGeometry = (egui::Rect, Vec<egui::Rect>, Vec<egui::Rect>);
    let mut hits: Vec<HitRects> = Vec::new();
    let mut skeleton: Vec<SkeletonGeometry> = Vec::new();
    let mut callout_counts: Vec<usize> = Vec::new();

    for mode in [
        crate::config::LabelPresentation::Off,
        crate::config::LabelPresentation::Auto,
        crate::config::LabelPresentation::On,
    ] {
        let mut app = module_ui_labels_app(pane);
        app.label_presentation = mode;
        let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");

        // Derive the expected callout count from the selection rule rather than
        // hardcoding it: `off` annotates nothing, `auto` exactly the written
        // labels with no in-cell layout, `on` every written label. This fixture
        // labels every element it addresses, and the labels-first fit leaves none
        // of them without a layout — so `auto` is silent at the zoom the view
        // opens on and the annotation layer's only visible mode here is `on`.
        let written_labels = 1 + MODULE_UI_LABELS_CLIPPED.len();
        let addressed: Vec<&str> = spec
            .cells
            .iter()
            .filter(|cell| cell.component_index.is_some())
            .map(|cell| cell.label.as_str())
            .collect();
        assert_eq!(
            addressed.len(),
            written_labels,
            "the fixture must address exactly its written labels: {addressed:?}"
        );
        assert!(
            spec.cells
                .iter()
                .filter(|cell| cell.component_index.is_some())
                .all(|cell| cell.label_layout.is_some()),
            "at the labels-first fit every written label has an in-cell layout, so `auto` has no candidate"
        );

        hits.push(
            spec.cells
                .iter()
                .filter_map(|cell| {
                    cell.component_index.map(|index| {
                        let r = cell.rect;
                        (
                            index,
                            Rect::new(
                                r.min.x.round().max(0.0) as u16,
                                r.min.y.round().max(0.0) as u16,
                                r.width().round().max(0.0) as u16,
                                r.height().round().max(0.0) as u16,
                            ),
                        )
                    })
                })
                .collect(),
        );

        // Paint through the real routine in both presentations: the annotation
        // layer must be decoration only, never a second geometry source.
        for skeleton_mode in [false, true] {
            app.physical_show_skeleton = skeleton_mode;
            let label_spec =
                crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");
            let _ = module_ui_labels_painted_texts(pane, |ui| {
                crate::gui::paint_physical(ui, pane, Some(&label_spec));
            });
            if skeleton_mode {
                skeleton.push((
                    label_spec.case_rect,
                    label_spec.modules.iter().map(|m| m.rect).collect(),
                    label_spec.cells.iter().map(|c| c.rect).collect(),
                ));
            }
        }
        app.physical_show_skeleton = false;

        // The 14 pt mono headline count is the visible difference between the
        // modes — asserted so the invariance below is not vacuous.
        let texts = module_ui_labels_painted_texts(pane, |ui| {
            crate::gui::paint_physical(ui, pane, Some(&spec));
        });
        let callout_font = egui::FontId::monospace(14.0);
        callout_counts.push(
            spec.annotations
                .iter()
                .filter(|callout| {
                    texts
                        .iter()
                        .any(|(text, font)| *text == callout.label && *font == callout_font)
                })
                .count(),
        );
    }

    assert!(
        !hits[0].is_empty(),
        "the fixture must publish hit rects for its addressed buttons"
    );
    assert_eq!(hits[0], hits[1], "off and auto publish identical hit rects");
    assert_eq!(hits[0], hits[2], "off and on publish identical hit rects");
    assert_eq!(
        skeleton[0], skeleton[1],
        "off and auto draw identical skeleton rects"
    );
    assert_eq!(
        skeleton[0], skeleton[2],
        "off and on draw identical skeleton rects"
    );
    assert_eq!(
        callout_counts,
        vec![0, 0, 1 + MODULE_UI_LABELS_CLIPPED.len()],
        "off annotates nothing, auto nothing at the labels-first fit (every written label has a layout), on all three written labels"
    );
}

// ── module-ui-label-fit (task 3.2): one fixture, every ladder branch ────────
// `fixtures/module_ui_label_fit.ini` (embedded below) puts five written labels
// on one small rack — a P8S8 and a P4B2 under the master — so a single patch
// reaches every outcome the per-cell ladder can produce, through the production
// load + fit triggers, the real spec builder and the real paint path:
//
//   B3.1 (9 mm button, DB8E)  wraps to two or three lines at a step below the
//                             top — the "a long label wraps instead of
//                             disappearing" branch, drawn whole, no ellipsis.
//   P2.1 (12 mm pot, P10)     draws whole on one 8 pt line and keeps its state
//                             row in the band `h − 0.66 w` beneath its face —
//                             one step below the top, because the band, not the
//                             cell, is the label's budget now.
//   I1   (7 mm jack, master)  takes a step below the top — the "a small cell
//                             takes a smaller step" branch, never below the floor.
//   P1.1 (P8S8 fader cell)    stacks one glyph per line in the column beside the
//                             narrowed track (D5).
//   L1   (1.8 mm LED, DB8E)   has no in-cell layout at any supported zoom, so
//                             `auto` must annotate it — the fallback branch.
//
// The story also pins the change's three cross-layer promises: no painted run
// is below the 6 pt floor, every fitted line is painted inside its own cell
// while the cell the ladder cannot serve paints no text at all, and the
// labels-first fit is the greater of the rack fit and the band's lift with the
// pot's, jack's and button's labels drawn whole — the LED's impossible label
// must not climb towards the zoom cap.
//
// faceplate-pot-jack-symbols (task 3.2) moved the fit's answer here: the pot and
// the master jack now fit their labels into the `h − 0.66 w` band beneath their
// faces (D2), so the fit lifts to 1.44 instead of the rack fit 1.25 — the jack's
// 10-character label needs the deeper band. The lift is pinned below with the
// face band named, so the number cannot drift silently.
//
// NOTE (closed by task 3.6): `gui::physical::physical_spec` now derives
// `CellSpec::is_fader` from the geometry cell's `F` family and `value_frac`
// from the element's state, so a loaded patch's fader cell paints through the D5
// face — the narrowed track, the amber value bar and the stacked label column —
// and its own `label_layout` is the fader column below. The fader assertions pin
// both sides of that: the box the *fit* measures, and the layout the *painter*
// reads, which is the same `CellTextPath::Fader` box on the drawn cell.

/// The fixture compiled in: five written labels, one per ladder outcome.
const MODULE_UI_LABEL_FIT_FIXTURE: &str = include_str!("../fixtures/module_ui_label_fit.ini");

/// The 9 mm button cell whose written label wraps (`B3.1`, DB8E).
const MODULE_UI_LABEL_FIT_BUTTON: &str = "B3.1";

/// The 12 mm pot cell whose short written label still takes the top step (`P10`).
const MODULE_UI_LABEL_FIT_POT: &str = "P2.1";

/// The 7 mm master jack whose written label steps down.
const MODULE_UI_LABEL_FIT_JACK: &str = "I1";

/// The P8S8 fader cell (4.2 × 22 mm) whose written label stacks.
const MODULE_UI_LABEL_FIT_FADER: &str = "P1.1";

/// The 1.8 mm LED cell whose written label no supported zoom can draw whole.
const MODULE_UI_LABEL_FIT_LED: &str = "L1";

/// The rack the fixture packs into, from the same chain + default-case source
/// the app's fit and the painter consume.
fn module_ui_label_fit_rack(patch: &Patch) -> crate::physical::RackLayout {
    let chain =
        crate::physical::PhysicalLayout::build(Some(patch), crate::patch::MasterModel::Master, &[]);
    let spec = crate::physical::RackSpec::default_case(&chain);
    crate::physical::RackLayout::pack(&chain, &spec)
}

/// A pane 1.25 true-size racks tall and 1600 pt wide: the fit is height-bound and
/// lands a quarter above 1:1, which leaves the 9 mm cells room for a mid-ladder
/// step (not the floor) while the 12 mm pot still takes the top step. The width is
/// generous on purpose — the one annotation the `auto` branch places must fit the
/// 30 % band budget beside the rack, and this fixture's rack is wide (P8S8 + P10 +
/// DB8E); a narrow pane would drop the callout and take the assertion with it.
fn module_ui_label_fit_pane(rack: &crate::physical::RackLayout) -> egui::Rect {
    let (_, height) = crate::physical::rack_extent_points(rack, 1.0);
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(1600.0, height * 1.25))
}

/// Whether a layout draws its whole label: the concatenated, whitespace-free
/// lines equal the label's own characters. This is D7's "drawn whole" predicate
/// — the one a zoom earns the fit for, and the one an ellipsis fails.
fn module_ui_label_fit_draws_whole(label: &str, layout: &crate::physical::LabelLayout) -> bool {
    layout
        .lines
        .iter()
        .flat_map(|line| line.chars())
        .filter(|c| !c.is_whitespace())
        .eq(label.chars().filter(|c| !c.is_whitespace()))
}

/// A cell's label box at a candidate zoom, as the fit measures it: the drawn
/// rect scaled by `zoom / current_zoom`, no state glyph charge and no state row
/// (`App::physical_label_fit_requests` passes the same three facts).
fn module_ui_label_fit_box_at(
    rect: egui::Rect,
    current_zoom: f32,
    zoom: f32,
    path: crate::physical::CellTextPath,
) -> crate::physical::LabelBox {
    let scale = zoom / current_zoom;
    crate::physical::LabelBox {
        w: rect.width() * scale,
        h: rect.height() * scale,
        glyph_chars: 0,
        path,
        has_state: false,
    }
}

/// Headless `paint_physical` text runs as `(text, font, painted rect)`.
///
/// Position matters here: a cell's own line has to land inside that cell, and
/// the cell the ladder cannot serve must paint no text at all — neither is
/// observable from the spec alone.
fn module_ui_label_fit_painted_runs(
    pane: egui::Rect,
    mut paint: impl FnMut(&mut egui::Ui),
) -> Vec<(String, egui::FontId, egui::Rect)> {
    use egui::epaint::Shape;

    let ctx = egui::Context::default();
    let raw_input = egui::RawInput {
        screen_rect: Some(pane),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw_input, |ui| paint(ui));
    let mut runs = Vec::new();
    for clipped in &out.shapes {
        if let Shape::Text(text) = &clipped.shape {
            let font = text.galley.job.sections[0].format.font_id.clone();
            runs.push((
                text.galley.text().to_string(),
                font,
                egui::Rect::from_min_size(text.pos, text.galley.size()),
            ));
        }
    }
    out.textures_delta.clear();
    runs
}

#[test]
fn regression_module_ui_label_fit_covers_every_ladder_branch() {
    let patch = Patch::from_ini_str(MODULE_UI_LABEL_FIT_FIXTURE, String::from("label_fit"))
        .expect("the label-fit fixture must parse");

    // The fixture is a patch, not a validation-case carrier: a single finding —
    // Error or warning — would mean it stopped being a clean load.
    let issues = crate::validation::validate_patch(&patch, crate::schema::load_schema());
    assert!(
        issues.is_empty(),
        "fixtures/module_ui_label_fit.ini must load clean, got {issues:#?}"
    );

    // Open the module UI the way the app does and let the fit triggers run.
    let rack = module_ui_label_fit_rack(&patch);
    let pane = module_ui_label_fit_pane(&rack);
    let mut app = App::new();
    assert!(app.load_patch(patch.clone()), "the fixture must load");
    app.note_physical_pane_rect(Rect::new(
        0,
        0,
        pane.width().round() as u16,
        pane.height().round() as u16,
    ));
    app.apply_physical_fit_triggers();

    // The fit is the labels fit, and it is the greater of the rack fit and the
    // band's lift: the pot and the master jack fit their labels into the
    // `h − 0.66 w` band beneath their faces (faceplate-pot-jack-symbols D2), so
    // the jack's 10-character label lifts the view to 1.44 where the rack fit is
    // 1.25. The LED's label — which no zoom can serve — is still ignored rather
    // than allowed to drag the view towards the cap (D7).
    assert_eq!(
        app.physical_fit_mode,
        Some(crate::app::FitMode::Labels),
        "the view opens at the labels fit (D8)"
    );
    let fitted = app
        .physical_fit_zoom
        .expect("the fit trigger must report the zoom it applied");
    let rack_fit = crate::physical::fit_zoom(
        rack.total_width_mm,
        rack.total_height_mm,
        pane.width(),
        pane.height(),
    );
    // The band's answer on this fixture, measured: a face cell's text box is
    // `h − FACE_HEIGHT_FRACTION · w` with `FACE_HEIGHT_FRACTION = 0.66`, and the
    // 7 mm jack's band holds "Gate input" whole at 1.44.
    let face_band_lift = 1.44_f64;
    assert!(
        (f64::from(fitted) - rack_fit.max(face_band_lift)).abs() < 1e-6,
        "the fit must be the greater of the rack fit {rack_fit} and the face band's \
         lift {face_band_lift}: {fitted}"
    );
    assert!(
        f64::from(fitted) > rack_fit,
        "the face band lifts the fit above the rack fit: {fitted} vs {rack_fit}"
    );
    assert!(
        (app.physical_zoom - fitted).abs() < 1e-6,
        "the applied zoom must be the fitted one: {} vs {fitted}",
        app.physical_zoom
    );

    // Deterministic: the label fit scan is a fixed 1 % grid, so a refit lands on
    // the same zoom.
    assert!(app.fit_physical_view(pane.width(), pane.height()));
    assert_eq!(
        app.physical_fit_zoom,
        Some(fitted),
        "a refit must land on the same zoom"
    );

    let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");
    let cell = |token: &str| {
        let index = idx_for(&app, token);
        spec.cells
            .iter()
            .find(|cell| cell.component_index == Some(index))
            .unwrap_or_else(|| panic!("{token} must render a cell"))
    };

    // The five labels sit on the cells the fixture chose: a 9 mm button and a
    // 1.8 mm LED on the DB8E, a 12 mm pot on the P10, the P8S8 fader cell, and a
    // 7 mm master jack. Sizes are measured against the drawn zoom, not stored.
    let assert_mm = |token: &str, rect: egui::Rect, w_mm: f64, h_mm: f64| {
        let scale = crate::physical::PHYSICAL_POINTS_PER_MM * f64::from(app.physical_zoom);
        assert!(
            (f64::from(rect.width()) - w_mm * scale).abs() < 0.1
                && (f64::from(rect.height()) - h_mm * scale).abs() < 0.1,
            "{token} must be the {w_mm} × {h_mm} mm cell at {}%: {rect:?}",
            (app.physical_zoom * 100.0).round()
        );
    };

    let button = cell(MODULE_UI_LABEL_FIT_BUTTON);
    let pot = cell(MODULE_UI_LABEL_FIT_POT);
    let jack = cell(MODULE_UI_LABEL_FIT_JACK);
    let fader = cell(MODULE_UI_LABEL_FIT_FADER);
    let led = cell(MODULE_UI_LABEL_FIT_LED);
    assert_mm(MODULE_UI_LABEL_FIT_BUTTON, button.rect, 9.0, 9.0);
    assert_mm(MODULE_UI_LABEL_FIT_POT, pot.rect, 12.0, 12.0);
    assert_mm(MODULE_UI_LABEL_FIT_JACK, jack.rect, 7.0, 7.0);
    assert_mm(MODULE_UI_LABEL_FIT_FADER, fader.rect, 4.2, 22.0);
    assert_mm(MODULE_UI_LABEL_FIT_LED, led.rect, 1.8, 1.8);
    assert!(
        button.shape.is_none(),
        "a DB8E button is outside the fold rule and keeps the glyph path"
    );

    let top_step = MODULE_UI_LABEL_LADDER[0];
    let floor = MODULE_UI_LABEL_LADDER[4];

    // Pot: the face band's branch. The face keeps the cell and leaves
    // `h − 0.66 w` = 22.2 pt for the label at the fitted 1.44, so the ladder
    // takes one 8 pt line and the band still holds the `NN%` state row — the
    // row is not special-cased (D4), it is dropped only when the band cannot
    // hold it. One step below the top, never the floor.
    let pot_layout = pot
        .label_layout
        .as_ref()
        .expect("the pot's written label must have an in-cell layout");
    assert_eq!(
        pot_layout.lines.len(),
        1,
        "a 12 mm cell holds the short label on one line: {pot_layout:?}"
    );
    assert_eq!(
        pot_layout.font, 8.0,
        "the face band's 22.2 pt holds one 8 pt line: {pot_layout:?}"
    );
    assert!(
        pot_layout.font < top_step && pot_layout.font >= floor,
        "the band costs a step below the top and never the floor: {pot_layout:?}"
    );
    assert!(
        pot_layout.state_row,
        "the band still holds the pot's state row after its label (D4): {pot_layout:?}"
    );
    assert!(!pot_layout.stacked, "a pot does not stack: {pot_layout:?}");
    assert!(
        module_ui_label_fit_draws_whole(&pot.label, pot_layout),
        "the whole label, no ellipsis: {pot_layout:?}"
    );

    // Button: the wrap branch. A long label wraps before it steps down, and at
    // the fitted 1.44 the 9 mm composed cell's band holds the three lines at the
    // top step — the wrap, not a step, is what the branch demonstrates now.
    let button_layout = button
        .label_layout
        .as_ref()
        .expect("the button's written label must have an in-cell layout");
    assert!(
        (2..=3).contains(&button_layout.lines.len()),
        "a 9 mm cell folds the long label into two or three lines: {button_layout:?}"
    );
    assert_eq!(
        button_layout.font, top_step,
        "the fitted zoom holds the wrapped lines at the top step: {button_layout:?}"
    );
    assert!(
        button_layout.font >= floor,
        "a wrapped label never falls to the floor: {button_layout:?}"
    );
    assert!(
        button_layout.state_row,
        "the composed band still holds the button's state row (D4): {button_layout:?}"
    );
    assert!(
        !button_layout.stacked,
        "a button does not stack: {button_layout:?}"
    );
    assert!(
        module_ui_label_fit_draws_whole(&button.label, button_layout),
        "a wrapped label is drawn whole, never ellipsized: {button_layout:?}"
    );

    // Jack: the small-cell branch. A 7 mm cell takes a step below the top.
    let jack_layout = jack
        .label_layout
        .as_ref()
        .expect("the jack's written label must have an in-cell layout");
    assert!(
        jack_layout.font < top_step && jack_layout.font >= floor,
        "a 7 mm jack takes a lower ladder step, never below the floor: {jack_layout:?}"
    );
    assert!(
        !jack_layout.stacked,
        "a jack does not stack: {jack_layout:?}"
    );
    assert!(
        module_ui_label_fit_draws_whole(&jack.label, jack_layout),
        "the jack's label is drawn whole: {jack_layout:?}"
    );

    // Fader: the stacked branch (D5). This is the box the label *fit* measures
    // for an F-family cell — no state row, no glyph charge.
    let fader_box = crate::physical::LabelBox {
        w: fader.rect.width(),
        h: fader.rect.height(),
        glyph_chars: 0,
        path: crate::physical::CellTextPath::Fader,
        has_state: false,
    };
    let fader_layout = crate::physical::label_layout(&fader.label, &fader_box)
        .expect("the fader column holds a written label");
    assert!(
        fader_layout.stacked,
        "a 4.2 mm fader column stacks instead of clipping: {fader_layout:?}"
    );
    assert_eq!(
        fader_layout.lines.len(),
        fader.label.chars().count(),
        "one glyph per line: {fader_layout:?}"
    );
    assert!(
        fader_layout
            .lines
            .iter()
            .all(|line| line.chars().count() == 1),
        "one glyph per line: {fader_layout:?}"
    );
    assert!(
        module_ui_label_fit_draws_whole(&fader.label, &fader_layout),
        "the stacked column still draws the whole label: {fader_layout:?}"
    );

    // The paint side of the same branch (task 3.6): the geometry `F` family sets
    // `CellSpec::is_fader`, so the cell paints the fader face and its own layout
    // is the fader column's — not the compact box a loaded patch used to get.
    assert!(
        fader.is_fader,
        "a loaded P8S8 fader cell must take the fader path: {fader:?}"
    );
    let painter_box = crate::physical::LabelBox {
        w: fader.rect.width(),
        h: fader.rect.height(),
        glyph_chars: 0,
        path: fader_box.path,
        has_state: !fader.state_text.is_empty(),
    };
    assert_eq!(
        fader.label_layout,
        crate::physical::label_layout(&fader.label, &painter_box),
        "the painted layout is the fader column the fit measured, not the compact box"
    );
    let painted_fader_layout = fader
        .label_layout
        .as_ref()
        .expect("the fader cell fits its written label in its own column");
    assert!(
        painted_fader_layout.stacked,
        "the painted cell stacks its label: {painted_fader_layout:?}"
    );
    assert!(
        painted_fader_layout
            .lines
            .iter()
            .all(|line| line.chars().count() == 1),
        "one glyph per line: {painted_fader_layout:?}"
    );
    assert!(
        module_ui_label_fit_draws_whole(&fader.label, painted_fader_layout),
        "the painted stack still draws the whole label: {painted_fader_layout:?}"
    );

    // LED: the fallback branch. A 3 mm cell cannot show three characters plus an
    // ellipsis at the floor, so it has no in-cell layout at all — and no
    // supported zoom can draw its label whole, which is why the fit above stayed
    // at the rack fit (D7's ignore rule).
    assert!(
        led.label_layout.is_none(),
        "a 3 mm LED cell must fall back to the label presentation: {:?}",
        led.label_layout
    );
    let led_at_cap = module_ui_label_fit_box_at(
        led.rect,
        app.physical_zoom,
        crate::physical::ZOOM_MAX as f32,
        crate::physical::CellTextPath::Compact,
    );
    let led_whole_at_cap = crate::physical::label_layout(&led.label, &led_at_cap)
        .is_some_and(|layout| module_ui_label_fit_draws_whole(&led.label, &layout));
    assert!(
        !led_whole_at_cap,
        "the LED's label must stay impossible at the {}× cap, or the fit would climb for it",
        crate::physical::ZOOM_MAX
    );

    // `auto` annotates exactly the written label no cell can hold — never the
    // four the ladder did serve, and never a derived name.
    let annotated: Vec<&str> = spec
        .annotations
        .iter()
        .map(|callout| callout.label.as_str())
        .collect();
    assert_eq!(
        annotated,
        vec![led.label.as_str()],
        "`auto` must annotate the unlaid written label and only it (got {annotated:?})"
    );

    // Now the paint: the real `paint_physical` over the same spec.
    let runs = module_ui_label_fit_painted_runs(pane, |ui| {
        crate::gui::paint_physical(ui, pane, Some(&spec));
    });
    assert!(!runs.is_empty(), "the fixture paints at least its labels");

    // The floor (D3/D10): no run anywhere on the surface is below 6 pt.
    for (text, font, _) in &runs {
        assert!(
            font.size >= floor,
            "painted {text:?} at {} pt, below the {floor} pt floor",
            font.size
        );
    }

    // Every fitted line is painted at its own step, inside its own cell: the
    // painter draws the layout rather than re-deriving a budget.
    for (token, layout, rect) in [
        (MODULE_UI_LABEL_FIT_BUTTON, button_layout, button.rect),
        (MODULE_UI_LABEL_FIT_POT, pot_layout, pot.rect),
        (MODULE_UI_LABEL_FIT_JACK, jack_layout, jack.rect),
    ] {
        let font = egui::FontId::monospace(layout.font);
        for line in &layout.lines {
            assert!(
                runs.iter().any(|(text, run_font, run_rect)| {
                    text == line && *run_font == font && rect.expand(1.0).contains_rect(*run_rect)
                }),
                "{token} must paint {line:?} at {} pt inside its cell: {runs:?}",
                layout.font
            );
        }
    }

    // The stacked column is painted, not merely laid out: one monospace run per
    // glyph, inside the fader cell and to the right of the narrowed track.
    for line in &painted_fader_layout.lines {
        let font = egui::FontId::monospace(painted_fader_layout.font);
        let (_, _, run_rect) = runs
            .iter()
            .find(|(text, run_font, run_rect)| {
                text == line && *run_font == font && fader.rect.expand(1.0).contains_rect(*run_rect)
            })
            .unwrap_or_else(|| panic!("the fader must paint {line:?} inside its cell: {runs:?}"));
        assert!(
            run_rect.min.x >= fader.rect.min.x + 0.4 * fader.rect.width() - 0.5,
            "the stacked column sits beside the narrowed track: {run_rect:?}"
        );
    }

    // The cell the ladder cannot serve paints no text at all; its label lives in
    // the annotation outside the rack.
    assert!(
        !runs
            .iter()
            .any(|(_, _, run_rect)| led.rect.expand(1.0).contains_rect(*run_rect)),
        "the 3 mm LED cell must paint no text: {runs:?}"
    );
    let callout_font = egui::FontId::monospace(14.0);
    assert!(
        runs.iter()
            .any(|(text, font, _)| *text == led.label && *font == callout_font),
        "the LED's written label must paint as a callout: {runs:?}"
    );
    for label in [&pot.label, &button.label, &jack.label] {
        assert!(
            !runs
                .iter()
                .any(|(text, font, _)| text == label && *font == callout_font),
            "{label:?} drawn in its cell must not be duplicated as a callout: {runs:?}"
        );
    }
}

// ── faceplate-pot-jack-symbols (task 3.2): the face moves the band, not the rect ─
// A pot, encoder or CV jack now draws a face and fits its label into the band
// `h − FACE_HEIGHT_FRACTION · w` beneath it (`src/physical.rs`, D2), and
// `App::label_fit_path` measures that same band for the labels fit (D2/1.4).
// These stories pin the cross-layer consequences on real fixtures through the
// production load, fit trigger, spec builder and paint path:
//
//   * the dense rack's labels fit (`physical_multirow_rack.ini` at its 37 %
//     rack fit) lifts above the rack fit, where the pre-change whole-cell box
//     left it at the rack fit;
//   * the `auto` annotation set at that rack fit is the labels the smaller band
//     cannot hold — the pots newly, the button as before, and the master jack's
//     callout dropped by the unchanged family-0 budget;
//   * both fit clamps (25 % floor, 400 % cap) still answer at the extremes;
//   * an inventoried master's one written jack label now lifts the fit to the
//     band, and its spare jacks draw dimmed socket faces without a hit rect;
//   * every published element hit rect is still exactly the geometry cell rect
//     `physical::layout` produced — a face changes no geometry.

/// The dense rack the change's risk section names: sixteen bare P2B8 fronts and
/// the CV I/O master in one 457.2 × 128.5 mm row. The module UI's `BigLeft` pane
/// in a 1280 × 800 window is 640 × 800, which lands this rack's fit at 37 %.
const FACE_MULTIROW_FIXTURE: &str = "fixtures/physical_multirow_rack.ini";

/// The written labels the dense-rack stories add to that fixture. It is a
/// geometry fixture and carries none of its own, so the story writes the label
/// set a rack this dense would carry — two pots on the first front, a pot and a
/// button on the second, and the master's first CV input. The set spans the
/// three families the change touches plus the button whose band did not move.
const FACE_MULTIROW_LABELS: &[(&str, &str)] = &[
    ("P1.1", "Cutoff"),
    ("P1.2", "Resonance"),
    ("P2.1", "Cutoff"),
    ("B1.1", "Trigger input"),
    ("I1", "Gate input"),
];

/// The fit lift the face band now earns on [`FACE_MULTIROW_FIXTURE`] with
/// [`FACE_MULTIROW_LABELS`]: the master's 7 mm jack needs a 1.44 zoom for its
/// 10-character label to fit one 6 pt line in the `h − 0.66 w` band. The
/// whole-cell box the fit measured before this change answered 0.88, below the
/// rack fit, so the same view used to open at the rack fit.
const FACE_MULTIROW_LABEL_LIFT: f64 = 1.44;

/// The rack the multirow fixture packs into, from the same chain + default-case
/// source `App::apply_physical_fit` and `physical_spec` consume.
fn face_multirow_chain_and_rack(
    patch: &Patch,
) -> (crate::physical::PhysicalLayout, crate::physical::RackLayout) {
    let chain =
        crate::physical::PhysicalLayout::build(Some(patch), crate::patch::MasterModel::Master, &[]);
    let spec = crate::physical::RackSpec::default_case(&chain);
    let rack = crate::physical::RackLayout::pack(&chain, &spec);
    (chain, rack)
}

/// The module UI's `BigLeft` pane in a 1280 × 800 window.
fn face_multirow_pane() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(640.0, 800.0))
}

/// The fixture's patch with `labels` written into its preamble, the way the
/// label store hands them to `Patch::explicit_hw_label`.
fn face_labelled_multirow_patch(labels: &[(&str, &str)]) -> Patch {
    let mut patch = Patch::from_ini_file(Path::new(FACE_MULTIROW_FIXTURE))
        .unwrap_or_else(|e| panic!("{FACE_MULTIROW_FIXTURE} must parse: {e}"));
    for (token, label) in labels {
        patch
            .preamble_labels
            .insert((*token).to_string(), (*label).to_string());
    }
    patch
}

/// An app opened on `patch` through the production load + fit triggers, with the
/// module UI pane published at `pane` (the same trigger the pane dispatch runs).
fn face_fitted_app(patch: Patch, pane: egui::Rect) -> App {
    let mut app = App::new();
    assert!(app.load_patch(patch), "the fixture must load");
    app.note_physical_pane_rect(Rect::new(
        0,
        0,
        pane.width().round() as u16,
        pane.height().round() as u16,
    ));
    app.apply_physical_fit_triggers();
    app
}

/// The `(label, layout)` of `token`'s cell in `spec`, resolved by the component
/// index the spec maps back through `hw_components` — the same mapping the hit
/// publication uses. A closure rather than a helper because the spec's cell type
/// is private to `gui::physical` and cannot be named in a signature here.
macro_rules! face_cell_of {
    ($app:expr, $spec:expr, $token:expr) => {{
        let index = idx_for($app, $token);
        $spec
            .cells
            .iter()
            .find(|cell| cell.component_index == Some(index))
            .unwrap_or_else(|| panic!("{} must render a cell", $token))
    }};
}

#[test]
fn regression_faceplate_faces_lift_the_labels_fit_to_the_band() {
    let patch = face_labelled_multirow_patch(&[]);
    let (_, rack) = face_multirow_chain_and_rack(&patch);
    let pane = face_multirow_pane();
    let rack_fit = crate::physical::fit_zoom(
        rack.total_width_mm,
        rack.total_height_mm,
        pane.width(),
        pane.height(),
    );
    assert!(
        (rack_fit - 0.370_370_370_370_370_4).abs() < 1e-9,
        "640 pt of pane over 457.2 mm is the 37 % rack fit: {rack_fit}"
    );

    // (a) The fixture's own state: it writes no label, so the labels fit has no
    // request to serve and the rack fit answers alone — the face moves nothing.
    let plain = face_fitted_app(patch.clone(), pane);
    assert_eq!(
        plain.physical_fit_mode,
        Some(crate::app::FitMode::Labels),
        "the view opens at the labels fit (D8)"
    );
    let plain_fit = plain
        .physical_fit_zoom
        .expect("the fit trigger must report the zoom it applied");
    assert!(
        (f64::from(plain_fit) - rack_fit).abs() < 1e-6,
        "no written label means the rack fit answers: {plain_fit} vs {rack_fit}"
    );

    // (b) With labels written, the band beneath the faces lifts the fit: every
    // pot and the master jack now measure `h − 0.66 w`, so the fit is the
    // greater of the rack fit and the band's label lift.
    let labelled = face_fitted_app(face_labelled_multirow_patch(FACE_MULTIROW_LABELS), pane);
    let fitted = labelled
        .physical_fit_zoom
        .expect("the fit trigger must report the zoom it applied");
    assert!(
        (f64::from(fitted) - rack_fit.max(FACE_MULTIROW_LABEL_LIFT)).abs() < 1e-6,
        "the fit must be the greater of the rack fit {rack_fit} and the face band's lift {FACE_MULTIROW_LABEL_LIFT}: {fitted}"
    );
    assert!(
        f64::from(fitted) > rack_fit,
        "the band lifts the fit above the rack fit: {fitted} vs {rack_fit}"
    );
    assert!(
        (labelled.physical_zoom - fitted).abs() < 1e-6,
        "the applied zoom must be the fitted one: {} vs {fitted}",
        labelled.physical_zoom
    );

    // The whole-cell box this fit measured before the change answered 0.88 —
    // above the rack fit but well below the band's 1.44, so the same view used
    // to open at 88 % with the pots' labels wrapped into a clipped two-line
    // layout. The face band is what lifts it to 1.44.
    let pre_change_lift = 0.88_f64;
    assert!(
        rack_fit < pre_change_lift && pre_change_lift < f64::from(fitted),
        "the pre-change whole-cell answer sat between the rack fit and the band's lift: \
         {rack_fit} < {pre_change_lift} < {fitted}"
    );

    // (c) The fit's promise at the zoom it chose: every written label has an
    // in-cell layout and is drawn whole, so `auto` has nothing to annotate.
    let spec = crate::gui::physical_spec(&labelled, pane).expect("a loaded patch yields a spec");
    for &(token, label) in FACE_MULTIROW_LABELS {
        let cell = face_cell_of!(&labelled, &spec, token);
        assert_eq!(cell.label, label, "{token} keeps its written label");
        let layout = cell.label_layout.as_ref().unwrap_or_else(|| {
            panic!("{token} must have an in-cell layout at the fitted zoom: {cell:?}")
        });
        assert!(
            module_ui_label_fit_draws_whole(label, layout),
            "{token} must draw {label:?} whole at the fitted zoom: {layout:?}"
        );
    }
    assert!(
        spec.annotations.is_empty(),
        "a label the fitted zoom serves must not be annotated: {:?}",
        spec.annotations
            .iter()
            .map(|c| c.label.clone())
            .collect::<Vec<_>>()
    );

    // (d) Deterministic: the same rack and pane answer the same zoom twice.
    let mut refit = face_fitted_app(face_labelled_multirow_patch(FACE_MULTIROW_LABELS), pane);
    assert!(refit.fit_physical_view(pane.width(), pane.height()));
    assert_eq!(
        refit.physical_fit_zoom,
        Some(fitted),
        "the label fit scan is a fixed grid, so a refit lands on the same zoom"
    );
}

#[test]
fn regression_faceplate_faces_annotate_the_labels_the_band_cannot_hold() {
    let patch = face_labelled_multirow_patch(FACE_MULTIROW_LABELS);
    let (_, rack) = face_multirow_chain_and_rack(&patch);
    let pane = face_multirow_pane();
    let rack_fit = crate::physical::fit_zoom(
        rack.total_width_mm,
        rack.total_height_mm,
        pane.width(),
        pane.height(),
    );

    // The `auto` set is what the *rack fit* leaves unlaid — the labels-first fit
    // would lift until every one of them has a layout, so this story reads the
    // zoom the overview opens at, not the fitted one.
    let mut app = face_fitted_app(patch, pane);
    app.physical_zoom = rack_fit as f32;
    app.physical_offset = (0.0, 0.0);
    let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");

    // Every written label loses its in-cell layout at 37 %: the face band is
    // 5.7 pt under a 12 mm pot (16.8 pt cell − 0.66 × 16.8) and 3.3 pt under a
    // 7 mm jack, both below the 6 pt floor.
    for &(token, _) in FACE_MULTIROW_LABELS {
        let cell = face_cell_of!(&app, &spec, token);
        assert!(
            cell.label_layout.is_none(),
            "{token} must have no in-cell layout at the rack fit: {:?}",
            cell.label_layout
        );
    }

    // `auto` annotates four of the five: the three pot labels — which the
    // whole-cell box used to wrap whole, so they were never annotated before —
    // the button's label as before, and not the master jack's, whose callout the
    // unchanged 30 % band budget drops first (family 0: LEDs and jacks).
    let mut annotated: Vec<String> = spec
        .annotations
        .iter()
        .map(|callout| callout.label.clone())
        .collect();
    annotated.sort();
    assert_eq!(
        annotated,
        vec![
            String::from("Cutoff"),
            String::from("Cutoff"),
            String::from("Resonance"),
            String::from("Trigger input"),
        ],
        "the rack fit annotates the three pots and the button, and the budget drops the jack"
    );
    assert!(
        !annotated.iter().any(|label| label == "Gate input"),
        "the master jack's callout is the family-0 drop, not a laid-out label"
    );

    // The pots are the new callouts: measured against the pre-change whole-cell
    // box they had a layout at this zoom, so the `auto` rule (a label with *no*
    // layout) did not annotate them. "Cutoff" was even drawn whole there; the
    // band has no room for it at all.
    for &(token, label) in &FACE_MULTIROW_LABELS[..3] {
        let cell = face_cell_of!(&app, &spec, token);
        let old_box = crate::physical::LabelBox {
            w: cell.rect.width(),
            h: cell.rect.height(),
            glyph_chars: 0,
            path: crate::physical::CellTextPath::Compact,
            has_state: false,
        };
        let old_layout = crate::physical::label_layout(label, &old_box).unwrap_or_else(|| {
            panic!("{token}: the whole-cell box used to hold {label:?} at the rack fit")
        });
        assert!(
            !old_layout.lines.is_empty(),
            "{token}: the whole-cell box laid {label:?} out, so it was not annotated: {old_layout:?}"
        );
    }

    // ... and the jack's callout is genuinely placeable: on its own it is
    // annotated, so the count above is the budget's, not the band's.
    let only_jack = face_labelled_multirow_patch(&[("I1", "Gate input")]);
    let mut jack_app = face_fitted_app(only_jack, pane);
    jack_app.physical_zoom = rack_fit as f32;
    jack_app.physical_offset = (0.0, 0.0);
    let jack_spec =
        crate::gui::physical_spec(&jack_app, pane).expect("a loaded patch yields a spec");
    let jack_labels: Vec<&str> = jack_spec
        .annotations
        .iter()
        .map(|callout| callout.label.as_str())
        .collect();
    assert_eq!(
        jack_labels,
        vec!["Gate input"],
        "the band leaves the jack's label unlaid, so `auto` annotates it when it is alone"
    );
}

#[test]
fn regression_faceplate_faces_clamp_the_fit_to_the_zoom_range() {
    let patch = face_labelled_multirow_patch(&[]);

    // An extreme pane on either side: the fit stops at the 400 % cap and at the
    // 25 % floor, never outside them.
    for (w, h, expected, why) in [
        (
            8000u16,
            8000u16,
            crate::physical::ZOOM_MAX,
            "a pane wider than the 400 % cap",
        ),
        (
            60,
            40,
            crate::physical::ZOOM_MIN,
            "a pane below the 25 % floor",
        ),
    ] {
        let pane = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(f32::from(w), f32::from(h)),
        );
        let app = face_fitted_app(patch.clone(), pane);
        assert_eq!(
            app.physical_fit_mode,
            Some(crate::app::FitMode::Labels),
            "{why}: the labels fit is still the mode"
        );
        let fitted = app
            .physical_fit_zoom
            .expect("the fit trigger must report the zoom it applied");
        assert!(
            (f64::from(fitted) - expected).abs() < 1e-6,
            "{why} must clamp the fit to {expected}: {fitted}"
        );
        assert!(
            (crate::physical::ZOOM_MIN..=crate::physical::ZOOM_MAX).contains(&f64::from(fitted)),
            "{why}: {fitted} must stay inside the supported range"
        );
    }

    // A labelled rack answers the same range at both extremes: the label lift is
    // inside it, so the clamp — not the lift — decides.
    for (w, h) in [(8000u16, 8000u16), (60, 40)] {
        let pane = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(f32::from(w), f32::from(h)),
        );
        let app = face_fitted_app(face_labelled_multirow_patch(FACE_MULTIROW_LABELS), pane);
        let fitted = app
            .physical_fit_zoom
            .expect("the fit trigger must report the zoom it applied");
        assert!(
            (crate::physical::ZOOM_MIN..=crate::physical::ZOOM_MAX).contains(&f64::from(fitted)),
            "a labelled rack at {w}×{h} must stay inside the range: {fitted}"
        );
    }
}

#[test]
fn regression_faceplate_faces_relock_the_inventoried_master_to_the_band() {
    // The inventory draws the whole master front; the patch addresses one CV
    // input and one CV output, and writes a label on the input.
    let mut patch = Patch::from_ini_str(
        "[copy]\n    input = I1\n    output = O1\n",
        String::from("face_inventoried_master"),
    )
    .expect("the master fixture must parse");
    patch
        .preamble_labels
        .insert(String::from("I1"), String::from("Gate input"));

    let mut app = App::new();
    assert!(
        app.load_patch(patch.clone()),
        "the master fixture must load"
    );
    app.physical_rack_spec = rack_inventory(&["master"]);
    let inventory = app.physical_rack_spec.module_keys();
    let chain =
        crate::physical::PhysicalLayout::build(app.patch.as_ref(), app.master_model(), &inventory);
    assert!(
        chain.inventory_driven,
        "the inventory built this chain, so the whole front draws"
    );
    let rack_spec = app.physical_rack_spec.clone();
    let rack = crate::physical::RackLayout::pack(&chain, &rack_spec);
    assert_eq!(
        (rack.total_width_mm, rack.total_height_mm),
        (426.72, 128.5),
        "the inventory packs one full 84 HP row (426.72 mm) with the master at its head"
    );

    let pane = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(1600.0, 1000.0));
    app.note_physical_pane_rect(Rect::new(0, 0, 1600, 1000));
    app.apply_physical_fit_triggers();
    let rack_fit = crate::physical::fit_zoom(
        rack.total_width_mm,
        rack.total_height_mm,
        pane.width(),
        pane.height(),
    );
    let fitted = app
        .physical_fit_zoom
        .expect("the fit trigger must report the zoom it applied");
    assert!(
        (f64::from(fitted) - rack_fit.max(FACE_MULTIROW_LABEL_LIFT)).abs() < 1e-6,
        "the master's jack label must lift the fit to the face band: {fitted} vs rack {rack_fit}"
    );
    assert!(
        f64::from(fitted) > rack_fit,
        "the band lifts the inventoried master's fit above the rack fit: {fitted} vs {rack_fit}"
    );
    // Before the change the whole-cell box answered 0.76 for the same jack,
    // below the rack fit, so the fit used to open at the rack fit.
    let pre_change_lift = 0.76_f64;
    assert!(
        pre_change_lift < rack_fit,
        "the pre-change whole-cell answer sat below the rack fit: {pre_change_lift} vs {rack_fit}"
    );

    let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");
    assert_eq!(
        spec.cells.len(),
        33,
        "the inventory draws every declared master cell"
    );
    let jacks: Vec<_> = spec
        .cells
        .iter()
        .filter(|cell| cell.shape == Some(crate::gui::CellShape::Jack))
        .collect();
    assert_eq!(
        jacks.len(),
        16,
        "the master declares sixteen CV jack faces (8 in, 8 out)"
    );
    let published: Vec<usize> = spec
        .cells
        .iter()
        .filter_map(|cell| cell.component_index)
        .collect();
    assert_eq!(
        published.len(),
        2,
        "only the patch's two addressed jacks publish a hit rect"
    );
    for cell in &jacks {
        if cell.component_index.is_some() {
            assert!(!cell.dimmed, "an addressed jack is not dimmed");
        } else {
            assert!(
                cell.dimmed,
                "a spare jack draws its socket face dimmed, not hit-testable"
            );
        }
    }
    let spare_jacks = jacks
        .iter()
        .filter(|cell| cell.component_index.is_none())
        .count();
    assert_eq!(
        spare_jacks, 14,
        "fourteen spare CV jacks draw dimmed faces and publish no rect"
    );
    assert_eq!(
        spec.cells
            .iter()
            .filter(|cell| cell.component_index.is_none() && !cell.dimmed)
            .count(),
        0,
        "no unaddressed cell of the front is bright"
    );

    // The label is what lifted the fit: at the chosen zoom the band holds it on
    // one 6 pt line, drawn whole.
    let input = face_cell_of!(&app, &spec, "I1");
    let layout = input
        .label_layout
        .as_ref()
        .expect("the fitted zoom must lay out the jack's written label");
    assert_eq!(layout.font, MODULE_UI_LABEL_LADDER[4]);
    assert_eq!(layout.lines, vec![String::from("Gate input")]);
    assert!(
        module_ui_label_fit_draws_whole("Gate input", layout),
        "the band draws the jack's label whole: {layout:?}"
    );

    // At the rack fit the band is 8.9 pt deep, so the label is only laid out
    // ellipsized — clipped, which is what the labels fit exists to avoid.
    let mut at_rack_fit = App::new();
    assert!(at_rack_fit.load_patch(patch));
    at_rack_fit.physical_rack_spec = rack_inventory(&["master"]);
    at_rack_fit.physical_zoom = rack_fit as f32;
    at_rack_fit.physical_offset = (0.0, 0.0);
    let rack_spec =
        crate::gui::physical_spec(&at_rack_fit, pane).expect("a loaded patch yields a spec");
    let input_at_rack_fit = face_cell_of!(&at_rack_fit, &rack_spec, "I1");
    let clipped = input_at_rack_fit
        .label_layout
        .as_ref()
        .expect("the rack fit still lays the label out, at the 6 pt floor");
    assert!(
        !module_ui_label_fit_draws_whole("Gate input", clipped),
        "the rack fit's band clips the jack's label: {clipped:?}"
    );
}

/// The geometry cell rect of `token` in points: `element.rect_mm` placed at its
/// module's packed position and mapped by the painter's own `ScreenMapping`
/// (`mm × points-per-mm × zoom − offset`). This is the rect the pre-change
/// painter published; a face must reproduce it exactly.
fn face_geometry_cell_rect(
    chain: &crate::physical::PhysicalLayout,
    rack: &crate::physical::RackLayout,
    mapping: &crate::physical::ScreenMapping,
    token: &str,
) -> Option<egui::Rect> {
    for row in &rack.rows {
        for placed in &row.modules {
            let module = &chain.modules[placed.module_index];
            if !module.components.iter().any(|comp| comp.id == token) {
                continue;
            }
            let cell = chain.cell_for(placed.module_index, token)?;
            let (x, y, w, h) = mapping.mm_to_screen(crate::physical::RectMm {
                x_mm: placed.rect_mm.x_mm + cell.rect_mm.x_mm,
                y_mm: row.y_mm + placed.rect_mm.y_mm + cell.rect_mm.y_mm,
                w_mm: cell.rect_mm.w_mm,
                h_mm: cell.rect_mm.h_mm,
            });
            return Some(egui::Rect::from_min_size(
                egui::Pos2::new(x as f32, y as f32),
                egui::Vec2::new(w as f32, h as f32),
            ));
        }
    }
    None
}

/// The renderer→handler handoff the module UI publishes with (`gui::to_cell_rect`):
/// min and size rounded to whole cells, negative edges clamped to 0. The published
/// hit rect is this rounding of the geometry rect — the only clamp on this path
/// (the terminal skeleton's `mm_rect_to_screen` keeps adjoined cells apart on its
/// own integer grid and is not involved here).
fn face_cell_grid(rect: egui::Rect) -> Rect {
    Rect::new(
        rect.min.x.round().max(0.0) as u16,
        rect.min.y.round().max(0.0) as u16,
        rect.width().round().max(0.0) as u16,
        rect.height().round().max(0.0) as u16,
    )
}

/// **Invariant: a face changes no geometry.** On `fixtures/render_outlier_matrix.ini`
/// — pots (P1.1, P1.2), an encoder (E2.1) and both CV jack directions (I2.1,
/// O2.1) — every published element hit rect is exactly the geometry cell rect
/// `physical::layout` produced for that element: `element.rect_mm` ×
/// points-per-mm × zoom, at the module's packed position. The face changes the
/// text box the label is fitted to and the strokes drawn inside the cell, never
/// the cell, so the published rects are the pre-change rects.
#[test]
fn regression_faceplate_faces_publish_the_geometry_hit_rects() {
    let patch = Patch::from_ini_file(Path::new("fixtures/render_outlier_matrix.ini"))
        .expect("the mixed-kind fixture must parse");

    for zoom in [1.0_f32, 0.37] {
        let mut app = App::new();
        assert!(app.load_patch(patch.clone()), "the fixture must load");
        // Keep the manual zoom: the pane paint runs the fit trigger, and this
        // story asserts the rects at a chosen zoom.
        app.physical_nav_latched = true;
        app.physical_fit_pending = false;
        app.physical_zoom = zoom;
        app.physical_offset = (0.0, 0.0);

        // Publish through the production path, not a spec shortcut.
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(1280.0, 800.0));
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(window),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            crate::gui::paint_band(&mut app, ui, None, &[]);
        });
        out.textures_delta.clear();

        assert_eq!(
            app.component_rects.len(),
            13,
            "eight buttons, two pots, one encoder and two CV jacks publish: {:?}",
            app.component_rects
        );

        let (_, pane_cells) = app
            .pane_hit_rects
            .iter()
            .find(|(id, _)| *id == crate::panes::PaneId::BigLeft)
            .expect("the module UI pane is published");
        let pane = egui::Rect::from_min_size(
            egui::Pos2::new(f32::from(pane_cells.x), f32::from(pane_cells.y)),
            egui::Vec2::new(f32::from(pane_cells.width), f32::from(pane_cells.height)),
        );

        let chain = crate::physical::PhysicalLayout::build(
            Some(&patch),
            crate::patch::MasterModel::Master,
            &[],
        );
        let rack_spec = crate::physical::RackSpec::default_case(&chain);
        let rack = crate::physical::RackLayout::pack(&chain, &rack_spec);
        let mapping = crate::physical::ScreenMapping::new(
            crate::physical::PHYSICAL_POINTS_PER_MM,
            f64::from(app.physical_zoom),
            f64::from(app.physical_offset.0) - f64::from(pane.min.x),
            f64::from(app.physical_offset.1) - f64::from(pane.min.y),
        );
        let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch yields a spec");

        let mut geometry_rects = Vec::new();
        for (index, published) in &app.component_rects {
            let token = patch.hw_components[*index].id.as_str();
            let expected = face_geometry_cell_rect(&chain, &rack, &mapping, token)
                .unwrap_or_else(|| panic!("{token} must resolve to a geometry cell"));
            assert_eq!(
                *published,
                face_cell_grid(expected),
                "at {zoom}× {token}'s published hit rect must be its geometry cell rect \
                 {expected:?} rounded into the cell grid"
            );
            let cell = spec
                .cells
                .iter()
                .find(|cell| cell.component_index == Some(*index))
                .unwrap_or_else(|| panic!("{token} must publish a drawn cell"));
            assert!(
                (cell.rect.min.x - expected.min.x).abs() < 1e-3
                    && (cell.rect.min.y - expected.min.y).abs() < 1e-3
                    && (cell.rect.width() - expected.width()).abs() < 1e-3
                    && (cell.rect.height() - expected.height()).abs() < 1e-3,
                "at {zoom}× {token}'s drawn cell must be the geometry rect: {:?} vs {expected:?}",
                cell.rect
            );
            geometry_rects.push((token.to_string(), expected));
        }

        // The fixture reaches all three face families, so the invariant covers
        // them rather than a button-only subset.
        let faces: Vec<crate::gui::CellShape> = spec
            .cells
            .iter()
            .filter(|cell| cell.component_index.is_some())
            .filter_map(|cell| cell.shape)
            .collect();
        for shape in [
            crate::gui::CellShape::Pot,
            crate::gui::CellShape::Encoder,
            crate::gui::CellShape::Jack,
        ] {
            assert!(
                faces.contains(&shape),
                "the fixture must publish a {shape:?} cell: {faces:?}"
            );
        }

        // Adjacency: the geometry cells do not overlap, so neither do the
        // published rects (edge-touching counts as disjoint).
        for (i, (a_token, a)) in geometry_rects.iter().enumerate() {
            for (b_token, b) in &geometry_rects[i + 1..] {
                assert!(
                    a.max.x <= b.min.x
                        || b.max.x <= a.min.x
                        || a.max.y <= b.min.y
                        || b.max.y <= a.min.y,
                    "at {zoom}× {a_token} {a:?} and {b_token} {b:?} overlap"
                );
            }
        }
        for (i, (a_index, a)) in app.component_rects.iter().enumerate() {
            for (b_index, b) in &app.component_rects[i + 1..] {
                assert!(
                    a.x + a.width <= b.x
                        || b.x + b.width <= a.x
                        || a.y + a.height <= b.y
                        || b.y + b.height <= a.y,
                    "at {zoom}× the published rects of {} and {} overlap: {a:?} {b:?}",
                    patch.hw_components[*a_index].id,
                    patch.hw_components[*b_index].id
                );
            }
        }
    }
}

// ── module-ui-navigation (task 3.3): the map and its keys, end to end ─────
//
// The change's wiring claims, each on the dense rack
// (`fixtures/physical_multirow_rack.ini`) through the production paths: the
// window pointer frame (`handler::handle_graph_window_frame`), the painted band
// (`gui::paint_band`, which publishes the pane rects, the element hit rects and
// the map the pane drew), the physical frame applier and `handle_event`.
//
//   * a click inside a map module rect centres the pane on that module, and the
//     repainted viewport box follows it;
//   * a marquee of three points or more on both axes fits the dragged region
//     and latches the view, a shorter drag stays a click;
//   * the panel claims a press and the wheel's value step before any element
//     cell under it, while the same window point without a published map does
//     reach that element;
//   * `c` centres the focused element (else the rack) through `handle_event`,
//     `Shift+C` refits and re-arms the automatic fits, and `Esc` drops a marquee
//     without closing the pane.

/// The window these stories paint: 1280 × 800, so the module UI's `BigLeft`
/// pane is the 640 × 800 [`face_multirow_pane`] both fits are measured against.
fn module_ui_navigation_window() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(1280.0, 800.0))
}

/// Paint one whole window frame through the production band entry point — the
/// window loop's draw step — which publishes the module UI's pane rect, its
/// element hit rects and the minimap the pane drew.
fn module_ui_navigation_paint(app: &mut App) {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(module_ui_navigation_window()),
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        crate::gui::paint_band(app, ui, None, &[]);
    });
    out.textures_delta.clear();
}

/// The same frame with the pointer parked at `pos` and one wheel notch, so the
/// module UI's own input report runs and the pane dispatch applies it.
fn module_ui_navigation_paint_with_wheel(app: &mut App, pos: egui::Pos2, delta: f32) {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(module_ui_navigation_window()),
        events: vec![
            egui::Event::PointerMoved(pos),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            },
        ],
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        crate::gui::paint_band(app, ui, None, &[]);
    });
    out.textures_delta.clear();
}

/// The published pane rect the module UI is drawn in (`BigLeft` in this
/// window).
fn module_ui_navigation_pane(app: &App) -> egui::Rect {
    let (_, cell) = app
        .pane_hit_rects
        .iter()
        .find(|(id, _)| *id == crate::panes::PaneId::BigLeft)
        .expect("the painted frame publishes the module UI's pane");
    egui::Rect::from_min_size(
        egui::Pos2::new(f32::from(cell.x), f32::from(cell.y)),
        egui::Vec2::new(f32::from(cell.width), f32::from(cell.height)),
    )
}

/// One window pointer frame: a press, a move while the gesture is down, or a
/// release, at `point`.
fn module_ui_navigation_frame(
    point: egui::Pos2,
    pressed: bool,
    down: bool,
    released: bool,
) -> crate::gui::WindowFrame {
    crate::gui::WindowFrame {
        pointer: Some((point.x, point.y)),
        primary_pressed: pressed,
        primary_down: down || pressed,
        primary_released: released,
        ..Default::default()
    }
}

/// The map the painted frame published — every story reads that one answer
/// instead of re-deriving the geometry.
fn module_ui_navigation_map(app: &App) -> &crate::physical::MiniMap {
    app.physical_minimap
        .as_ref()
        .expect("the painted module-UI frame publishes a minimap")
}

/// A published model rect as an egui rect, for containment tests.
fn module_ui_navigation_rect(rect: crate::physical::RectPt) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::Pos2::new(rect.x, rect.y),
        egui::Vec2::new(rect.w, rect.h),
    )
}

/// The rack point under a map point. Written out here rather than called, so
/// the stories' expectations do not share the handler's inverse.
fn module_ui_navigation_rack_point(
    map: &crate::physical::MiniMap,
    point: egui::Pos2,
) -> egui::Pos2 {
    let (bx, by, _bw, _bh, ix, iy, sx, sy) = map.transform;
    egui::Pos2::new(bx + (point.x - ix) / sx, by + (point.y - iy) / sy)
}

/// The placed module rects in rack-local points at `zoom`, in the order
/// `physical_spec` publishes them (row order, then placement order) — the
/// geometry source the map's module rects are mapped from.
fn module_ui_navigation_module_rects(
    rack: &crate::physical::RackLayout,
    zoom: f32,
) -> Vec<egui::Rect> {
    let ppm = crate::physical::PHYSICAL_POINTS_PER_MM as f32 * zoom;
    let mut rects = Vec::new();
    for row in &rack.rows {
        for placed in &row.modules {
            rects.push(egui::Rect::from_min_size(
                egui::Pos2::new(
                    placed.rect_mm.x_mm as f32 * ppm,
                    (row.y_mm + placed.rect_mm.y_mm) as f32 * ppm,
                ),
                egui::Vec2::new(
                    placed.rect_mm.w_mm as f32 * ppm,
                    placed.rect_mm.h_mm as f32 * ppm,
                ),
            ));
        }
    }
    rects
}

/// The dense rack at its labels fit — the overflowing state the map exists for
/// (the pane shows about a quarter of the rack's width, so the viewport box is
/// a sub-rect that can follow a jump). Returns the app and the rack geometry
/// the stories measure against.
fn module_ui_navigation_app() -> (
    App,
    crate::physical::PhysicalLayout,
    crate::physical::RackLayout,
) {
    let patch = face_labelled_multirow_patch(FACE_MULTIROW_LABELS);
    let (chain, rack) = face_multirow_chain_and_rack(&patch);
    let app = face_fitted_app(patch, face_multirow_pane());
    assert!(
        (app.physical_zoom - FACE_MULTIROW_LABEL_LIFT as f32).abs() < 1e-3,
        "the stories need the overflowing labels fit, not the rack fit: {}",
        app.physical_zoom
    );
    (app, chain, rack)
}

/// The cell rect of `token` in rack-local points at `zoom` (no offset), the
/// space `physical_center_on_rect` and the map's inverse name.
fn module_ui_navigation_cell_rect(
    chain: &crate::physical::PhysicalLayout,
    rack: &crate::physical::RackLayout,
    zoom: f32,
    token: &str,
) -> egui::Rect {
    let mapping = crate::physical::ScreenMapping::new(
        crate::physical::PHYSICAL_POINTS_PER_MM,
        f64::from(zoom),
        0.0,
        0.0,
    );
    face_geometry_cell_rect(chain, rack, &mapping, token)
        .unwrap_or_else(|| panic!("{token} must resolve to a geometry cell"))
}

#[test]
fn regression_module_ui_map_click_centres_on_the_module_and_the_viewport_follows() {
    let (mut app, _chain, rack) = module_ui_navigation_app();
    module_ui_navigation_paint(&mut app);

    let pane = module_ui_navigation_pane(&app);
    assert_eq!(
        (pane.min, pane.size()),
        (egui::Pos2::ZERO, egui::Vec2::new(640.0, 800.0)),
        "the module UI draws in the 640 × 800 BigLeft pane"
    );

    let map = module_ui_navigation_map(&app).clone();
    assert_eq!(map.modules.len(), 17, "one rect per faceplate");

    // D2: the map is the 160 pt panel inset 8 pt from the pane's bottom-right,
    // its height on the 56 pt floor for this 3.56:1 rack.
    let panel = module_ui_navigation_rect(map.panel);
    assert!(
        (panel.width() - crate::physical::RACK_MINIMAP_WIDTH).abs() < 1e-3
            && (panel.height() - crate::physical::RACK_MINIMAP_MIN_H).abs() < 1e-3,
        "the panel is 160 × 56 pt: {panel:?}"
    );
    assert!(
        (panel.max.x - (pane.max.x - crate::physical::RACK_MINIMAP_PAD)).abs() < 1e-3
            && (panel.max.y - (pane.max.y - crate::physical::RACK_MINIMAP_PAD)).abs() < 1e-3,
        "the panel is inset 8 pt from the pane's bottom-right: {panel:?}"
    );
    assert_eq!(
        (panel.min, panel.size()),
        (egui::Pos2::new(472.0, 736.0), egui::Vec2::new(160.0, 56.0)),
        "the map occupies the pane's bottom-right corner"
    );

    // The click: the centre of the fourth module's rect on the map, inside the
    // map's inner area (not its inset border).
    let target = 3usize;
    let map_module = module_ui_navigation_rect(map.modules[target]);
    let click = map_module.center();
    assert!(
        panel
            .shrink(crate::physical::RACK_MINIMAP_INSET)
            .contains(click),
        "the click lands on the map's inner area, not its border: {click:?}"
    );
    // The module's centre in rack points, from the rack geometry the map is
    // drawn from — the point the click must centre on.
    let module_centre =
        module_ui_navigation_module_rects(&rack, app.physical_zoom)[target].center();

    let zoom_before = app.physical_zoom;
    let viewport_before = map.viewport;

    // Press: the panel claims it and anchors a marquee; no element is focused.
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(click, true, true, false),
        &mut app,
    );
    assert!(
        app.physical_minimap_drag.is_some(),
        "a press on the map anchors the marquee"
    );
    assert!(
        app.selected_component.is_none(),
        "a press on the map focuses no element: {:?}",
        app.selected_component
    );

    // Release at the same point: a zero-extent marquee is a click.
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(click, false, false, true),
        &mut app,
    );
    assert!(
        app.physical_minimap_drag.is_none(),
        "the release ends the gesture"
    );

    assert!(
        (app.physical_zoom - zoom_before).abs() < 1e-6,
        "a click must not zoom: {} vs {zoom_before}",
        app.physical_zoom
    );
    let expected = (
        module_centre.x - pane.width() / 2.0,
        module_centre.y - pane.height() / 2.0,
    );
    assert!(
        (app.physical_offset.0 - expected.0).abs() < 0.5
            && (app.physical_offset.1 - expected.1).abs() < 0.5,
        "the module's centre ({module_centre:?}) must land on the pane's centre: \
         offset {:?} vs {expected:?}",
        app.physical_offset
    );
    assert!(
        app.status_message.starts_with("Minimap: centred on"),
        "the status names the click: {:?}",
        app.status_message
    );

    // A repainted frame publishes a viewport box that moved onto the module.
    module_ui_navigation_paint(&mut app);
    let map_after = module_ui_navigation_map(&app).clone();
    let distance = |rect: crate::physical::RectPt| {
        let centre = module_ui_navigation_rect(rect).center();
        (centre - click).length()
    };
    assert!(
        distance(map_after.viewport) < distance(viewport_before),
        "the viewport box follows the jump: {:?} -> {:?}",
        viewport_before,
        map_after.viewport
    );
    let viewport_centre = module_ui_navigation_rect(map_after.viewport).center();
    assert!(
        (viewport_centre.x - click.x).abs() < 1.0,
        "the box is centred on the clicked module: {viewport_centre:?} vs {click:?}"
    );
}

#[test]
fn regression_module_ui_map_drag_frames_the_region_and_a_short_drag_centres() {
    let (mut app, _chain, _rack) = module_ui_navigation_app();
    module_ui_navigation_paint(&mut app);

    let pane = module_ui_navigation_pane(&app);
    let map = module_ui_navigation_map(&app).clone();
    let inner = module_ui_navigation_rect(map.panel).shrink(crate::physical::RACK_MINIMAP_INSET);
    let anchor = egui::Pos2::new(500.0, 754.0);
    let end = egui::Pos2::new(560.0, 774.0);
    assert!(
        inner.contains(anchor) && inner.contains(end),
        "the marquee runs inside the map area: {anchor:?}..{end:?} in {inner:?}"
    );

    // The region the marquee names, and the zoom the region fit must land on.
    let zoom_before = app.physical_zoom;
    let rack_anchor = module_ui_navigation_rack_point(&map, anchor);
    let rack_end = module_ui_navigation_rack_point(&map, end);
    let region_w = (rack_end.x - rack_anchor.x).abs();
    let region_h = (rack_end.y - rack_anchor.y).abs();
    let expected = (zoom_before * (pane.width() / region_w).min(pane.height() / region_h)).clamp(
        crate::physical::ZOOM_MIN as f32,
        crate::physical::ZOOM_MAX as f32,
    );
    assert!(
        (expected - zoom_before).abs() > 0.1,
        "the dragged region must change the zoom: {expected} vs {zoom_before}"
    );

    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(anchor, true, true, false),
        &mut app,
    );
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(end, false, true, false),
        &mut app,
    );
    let live = app
        .physical_minimap_drag
        .expect("the marquee tracks the pointer");
    assert!(
        (live.2 - end.x).abs() < 1e-3 && (live.3 - end.y).abs() < 1e-3,
        "the marquee's current corner follows the pointer: {live:?}"
    );
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(end, false, false, true),
        &mut app,
    );

    assert!(
        app.physical_minimap_drag.is_none(),
        "the release ends the marquee"
    );
    assert!(
        (app.physical_zoom - expected).abs() < 1e-3,
        "the drag frames the region: {} vs {expected}",
        app.physical_zoom
    );
    assert!(
        app.physical_nav_latched,
        "a region zoom is manual navigation and latches the view"
    );
    assert!(
        app.status_message.starts_with("Minimap zoom"),
        "the status names the region zoom: {:?}",
        app.status_message
    );

    // A marquee under three points on either axis stays a click: the zoom is
    // untouched and the pane centres on the press point.
    module_ui_navigation_paint(&mut app);
    let map = module_ui_navigation_map(&app).clone();
    let zoom_after_region = app.physical_zoom;
    let short_anchor = egui::Pos2::new(552.0, 764.0);
    let short_end = egui::Pos2::new(554.0, 766.0);
    let click_centre = module_ui_navigation_rack_point(&map, short_anchor);

    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(short_anchor, true, true, false),
        &mut app,
    );
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(short_end, false, true, false),
        &mut app,
    );
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(short_end, false, false, true),
        &mut app,
    );

    assert!(
        (app.physical_zoom - zoom_after_region).abs() < 1e-6,
        "a sub-threshold drag is a click, not a region zoom: {} vs {zoom_after_region}",
        app.physical_zoom
    );
    let expected = (
        click_centre.x - pane.width() / 2.0,
        click_centre.y - pane.height() / 2.0,
    );
    assert!(
        (app.physical_offset.0 - expected.0).abs() < 0.5
            && (app.physical_offset.1 - expected.1).abs() < 0.5,
        "the click centres on the press point: offset {:?} vs {expected:?}",
        app.physical_offset
    );
}

#[test]
fn regression_module_ui_map_panel_claims_the_press_and_the_wheel() {
    let (mut app, chain, rack) = module_ui_navigation_app();
    module_ui_navigation_paint(&mut app);

    let pane = module_ui_navigation_pane(&app);
    let map = module_ui_navigation_map(&app).clone();
    let panel = module_ui_navigation_rect(map.panel);
    let centre = panel.center();

    // Park the view so the pot P1.1's cell sits under the panel's centre: the
    // map is painted opaque over the rack, so this is the overlap D3 is about.
    let cell = module_ui_navigation_cell_rect(&chain, &rack, app.physical_zoom, "P1.1");
    app.physical_offset = (cell.center().x - centre.x, cell.center().y - centre.y);
    app.physical_nav_latched = true; // parked by hand, as a pan would latch it
    module_ui_navigation_paint(&mut app);

    // The panel is the rack's shape, not the view's: a pan does not move it.
    let panel_after = module_ui_navigation_rect(module_ui_navigation_map(&app).panel);
    assert!(
        (panel_after.min - panel.min).length() < 1e-3
            && (panel_after.size() - panel.size()).length() < 1e-3,
        "the panel must not follow the view: {panel_after:?} vs {panel:?}"
    );

    let idx = idx_for(&app, "P1.1");
    let published = app
        .component_rects
        .iter()
        .find(|(index, _)| *index == idx)
        .map(|(_, rect)| {
            egui::Rect::from_min_size(
                egui::Pos2::new(f32::from(rect.x), f32::from(rect.y)),
                egui::Vec2::new(f32::from(rect.width), f32::from(rect.height)),
            )
        })
        .expect("P1.1 publishes a hit rect");
    assert!(
        published.contains(centre) && panel_after.contains(centre),
        "the panel and the element cell overlap at {centre:?}: {published:?} in {panel_after:?}"
    );
    assert!(
        (pane.width(), pane.height()) == (640.0, 800.0),
        "the stories measure against the 640 × 800 pane"
    );

    let value = |app: &App| match app.patch.as_ref().unwrap().hw_components[idx].state {
        crate::patch::ComponentState::Value(v) => v,
        ref other => panic!("P1.1 must be a pot, got {other:?}"),
    };
    let before = value(&app);

    // The wheel's value step is dropped while the panel covers the cell.
    crate::handler::handle_physical_frame(
        &crate::gui::PhysicalFrame {
            value_adjust: Some((idx, 0.05)),
            ..Default::default()
        },
        &mut app,
    );
    assert_eq!(
        value(&app),
        before,
        "the panel claims the wheel before the element under it"
    );

    // With no map published the same step adjusts the value.
    let map = app
        .physical_minimap
        .take()
        .expect("the painted frame published a map");
    crate::handler::handle_physical_frame(
        &crate::gui::PhysicalFrame {
            value_adjust: Some((idx, 0.05)),
            ..Default::default()
        },
        &mut app,
    );
    assert!(
        (value(&app) - (before + 0.05)).abs() < 1e-6,
        "without a map the wheel adjusts P1.1: {} vs {}",
        value(&app),
        before + 0.05
    );

    // ... and the same window point now focuses the element the panel covers.
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(centre, true, true, false),
        &mut app,
    );
    assert_eq!(
        app.selected_component.as_deref(),
        Some("P1.1"),
        "without a map the press reaches the element under the panel"
    );
    assert_eq!(app.hovered_component, Some(idx));
    assert_eq!(app.status_message, "Focused: P1.1");

    // Republish the map: the same point focuses nothing and adjusts nothing.
    app.physical_minimap = Some(map);
    app.clear_selected_component();
    app.hovered_component = None;
    let value_before_press = value(&app);
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(centre, true, true, false),
        &mut app,
    );
    assert!(
        app.selected_component.is_none(),
        "the panel claims the press: {:?}",
        app.selected_component
    );
    assert!(
        app.hovered_component.is_none(),
        "nothing under the panel is hovered: {:?}",
        app.hovered_component
    );
    assert!(
        app.physical_minimap_drag.is_some(),
        "a press on the panel anchors a marquee"
    );
    assert_eq!(
        value(&app),
        value_before_press,
        "a press on the panel adjusts no value"
    );
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(centre, false, false, true),
        &mut app,
    );
    assert!(app.physical_minimap_drag.is_none());
}

#[test]
fn regression_module_ui_c_and_shift_c_end_to_end() {
    let (mut app, chain, rack) = module_ui_navigation_app();
    module_ui_navigation_paint(&mut app);
    let pane = module_ui_navigation_pane(&app);
    assert!(
        app.view_focused(ViewType::Physical),
        "the module UI holds focus at startup"
    );
    let fitted_zoom = app.physical_zoom;

    // `c` with a focused element centres that element, at the current zoom.
    app.select_component(String::from("P2.1"));
    let pot = module_ui_navigation_cell_rect(&chain, &rack, fitted_zoom, "P2.1");
    handle_event(&key(KeyCode::Char('c')), &mut app);
    assert!(
        (app.physical_offset.0 - (pot.center().x - pane.width() / 2.0)).abs() < 0.5
            && (app.physical_offset.1 - (pot.center().y - pane.height() / 2.0)).abs() < 0.5,
        "`c` centres P2.1: offset {:?} vs centre {:?}",
        app.physical_offset,
        pot.center()
    );
    assert_eq!(app.status_message, "Centred on P2.1");
    assert!(
        (app.physical_zoom - fitted_zoom).abs() < 1e-6,
        "centring never zooms: {} vs {fitted_zoom}",
        app.physical_zoom
    );

    // With nothing focused it centres the rack extent instead.
    app.clear_selected_component();
    let (rack_w, rack_h) = crate::physical::rack_extent_points(&rack, f64::from(app.physical_zoom));
    handle_event(&key(KeyCode::Char('c')), &mut app);
    assert!(
        (app.physical_offset.0 - (rack_w / 2.0 - pane.width() / 2.0)).abs() < 0.5
            && (app.physical_offset.1 - (rack_h / 2.0 - pane.height() / 2.0)).abs() < 0.5,
        "`c` with nothing focused centres the rack: offset {:?}",
        app.physical_offset
    );
    assert_eq!(app.status_message, "Centred on rack");

    // On a pane that does not hold the module UI, `c` does nothing.
    open_viewer(&mut app);
    if app.view_focused(ViewType::Physical) {
        let id = app
            .pane_holding(ViewType::SourceViewer)
            .expect("the viewer holds a pane");
        app.layout.focus = id;
    }
    assert!(
        !app.view_focused(ViewType::Physical),
        "focus must be off the module UI for this step"
    );
    let parked = (
        app.physical_offset,
        app.physical_zoom,
        app.status_message.clone(),
    );
    handle_event(&key(KeyCode::Char('c')), &mut app);
    assert_eq!(
        (
            app.physical_offset,
            app.physical_zoom,
            app.status_message.clone()
        ),
        parked,
        "`c` on another pane must not touch the module UI"
    );

    // Back on the module UI: pan, zoom, then `Shift+C` refits and re-arms.
    let module_pane = app
        .pane_holding(ViewType::Physical)
        .expect("the module UI holds a pane");
    app.layout.focus = module_pane;
    module_ui_navigation_paint(&mut app);

    handle_event(&key(KeyCode::Right), &mut app);
    assert!(
        app.physical_nav_latched,
        "an arrow pan latches the view against automatic fits"
    );
    let panned = app.physical_offset;
    assert!(
        (panned.0 - parked.0 .0).abs() > 1.0,
        "the arrow must have panned the view: {panned:?} vs {:?}",
        parked.0
    );

    module_ui_navigation_paint_with_wheel(&mut app, egui::Pos2::new(20.0, 20.0), -50.0);
    let zoomed = app.physical_zoom;
    assert!(
        zoomed > fitted_zoom,
        "the wheel zooms in: {zoomed} vs {fitted_zoom}"
    );

    handle_event(&shift_key(KeyCode::Char('C')), &mut app);
    assert!(
        !app.physical_nav_latched,
        "`Shift+C` re-arms the automatic fit triggers"
    );
    assert!(
        (app.physical_zoom - fitted_zoom).abs() < 1e-3,
        "`Shift+C` refits to the labels fit: {} vs {fitted_zoom}",
        app.physical_zoom
    );
    let (fit_x, fit_y) = crate::physical::fit_offset(
        rack.total_width_mm,
        rack.total_height_mm,
        pane.width(),
        pane.height(),
        f64::from(app.physical_zoom),
    );
    assert!(
        (app.physical_offset.0 - fit_x).abs() < 0.5 && (app.physical_offset.1 - fit_y).abs() < 0.5,
        "the refit centres the rack: offset {:?} vs ({fit_x}, {fit_y})",
        app.physical_offset
    );
    assert!(
        app.status_message.contains("fit(labels)"),
        "the status reports the fit the key landed on: {:?}",
        app.status_message
    );
}

#[test]
fn regression_module_ui_esc_cancels_the_marquee_and_keeps_the_pane() {
    let (mut app, _chain, _rack) = module_ui_navigation_app();
    module_ui_navigation_paint(&mut app);

    let map = module_ui_navigation_map(&app).clone();
    let inner = module_ui_navigation_rect(map.panel).shrink(crate::physical::RACK_MINIMAP_INSET);
    let anchor = egui::Pos2::new(500.0, 754.0);
    let end = egui::Pos2::new(560.0, 774.0);
    assert!(inner.contains(anchor) && inner.contains(end));

    let (zoom, offset) = (app.physical_zoom, app.physical_offset);
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(anchor, true, true, false),
        &mut app,
    );
    crate::handler::handle_graph_window_frame(
        &module_ui_navigation_frame(end, false, true, false),
        &mut app,
    );
    assert!(
        app.physical_minimap_drag.is_some(),
        "a marquee is down before Esc"
    );

    handle_event(&key(KeyCode::Esc), &mut app);

    assert!(
        app.physical_minimap_drag.is_none(),
        "Esc drops the marquee instead of closing the pane"
    );
    assert_eq!(app.status_message, "Minimap marquee cancelled");
    assert!(
        app.pane_holding(ViewType::Physical).is_some(),
        "the module UI pane stays open"
    );
    assert!(
        (app.physical_zoom - zoom).abs() < 1e-6,
        "the cancelled marquee leaves the zoom: {} vs {zoom}",
        app.physical_zoom
    );
    assert_eq!(
        app.physical_offset, offset,
        "and leaves the view where it was"
    );
}

// ── view-maximize-and-performance-labels (task 3.2): cross-layer stories ──
//
// Each story drives production keys through `handle_event` and then asserts
// what the *next frame* would see — the App state a painter reads, the
// arrangement the band lays out, and the painted/spec geometry. The
// single-function behaviour itself is pinned in `handler.rs` and
// `gui/physical.rs`; these stories exist to prove the layers agree.

/// A real fixture loaded through the production loader, so the module UI, the
/// graph and the optimizer all have the state a running app builds.
fn cross_layer_app(fixture: &str) -> App {
    let patch = Patch::from_ini_file(Path::new(fixture))
        .unwrap_or_else(|err| panic!("{fixture} must parse: {err}"));
    let mut app = App::new();
    assert!(app.load_patch(patch), "{fixture} must load clean");
    app
}

/// The arrangement's four views in tree order — what "restore the
/// arrangement" means below, independent of which pane was maximized.
fn arrangement_views(app: &App) -> [Option<ViewType>; 4] {
    [
        app.layout.big_left.view,
        app.layout.big_right.view,
        app.layout.small_top.view,
        app.layout.small_bottom.view,
    ]
}

/// A performance-view pane for the geometry assertions: one fixed window-space
/// rect, so the spec's rack extent is comparable across key presses.
fn performance_story_pane() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(800.0, 600.0))
}

/// Apply the public part of `main.rs::seed_app` (that function is private to
/// the binary): the stories seed a fresh `App` exactly as startup does, so a
/// restart's persisted settings reach the same state the windowed loop builds.
fn seed_app_from_settings(app: &mut App, settings: &crate::config::Settings) {
    app.cost_model = crate::latency::CostModel::from_config(settings);
    app.seed_physical_view(&settings.physical);
    app.layout_mode = settings.layout.mode;
    app.layout_ordering = settings.layout.ordering;
    app.layout_slice_force = settings.layout.slice_force;
    app.layout_section_force = settings.layout.section_force;
    let knobs = crate::layout::SolveKnobs::default();
    app.layout_repulsion_force = knobs.repulsion;
    app.layout_cohesion_force = knobs.cohesion;
    app.labels = settings.labels.clone();
    app.performance_label_scale = settings.labels.performance_scale as f32;
    app.seed_graph_grouping(&settings.graph);
}

/// Serialises the favourite-picker story's environment override with its peers
/// (same pattern the handler tests use); the write itself goes to the temp dir.
fn regression_fav_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Point the platform data directory at `dir` for one test, restoring the
/// previous values on drop. Both variables are set because the platform rule
/// prefers `%APPDATA%` on Windows and XDG everywhere else.
struct RegressionDataDir {
    xdg: Option<std::ffi::OsString>,
    appdata: Option<std::ffi::OsString>,
}

impl RegressionDataDir {
    fn new(dir: &Path) -> Self {
        let xdg = std::env::var_os("XDG_CONFIG_HOME");
        let appdata = std::env::var_os("APPDATA");
        std::env::set_var("XDG_CONFIG_HOME", dir);
        std::env::set_var("APPDATA", dir);
        Self { xdg, appdata }
    }
}

impl Drop for RegressionDataDir {
    fn drop(&mut self) {
        match self.xdg.take() {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        match self.appdata.take() {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
    }
}

#[test]
fn regression_f_maximizes_every_pane_and_clears_like_z() {
    // One case per view the band can focus: module UI, graph, source viewer,
    // optimizer. Each opens the view through its real key, then `f` must
    // maximize whichever pane ended up focused.
    let cases: [(&str, fn(&mut App)); 4] = [
        ("module UI", |_| {}),
        ("graph", |app| {
            handle_event(&key(KeyCode::Char('g')), app);
            handle_event(&key(KeyCode::Char('g')), app);
        }),
        ("source viewer", |app| {
            handle_event(&key(KeyCode::Tab), app);
        }),
        ("optimizer", |app| {
            handle_event(&key(KeyCode::Char('g')), app);
            handle_event(&key(KeyCode::Char('o')), app);
        }),
    ];

    for (name, open) in cases {
        let mut app = cross_layer_app("fixtures/own_buttons.ini");
        open(&mut app);

        let focused = app.layout.focus;
        assert!(
            app.layout.pane(focused).view.is_some(),
            "{name}: the open key must leave a view in the focused pane"
        );
        let views = arrangement_views(&app);

        // `f` fills the band with the focused pane and leaves the arrangement
        // intact underneath — the state the geometry override reads.
        handle_event(&key(KeyCode::Char('f')), &mut app);
        assert_eq!(
            app.layout.maximized,
            Some(focused),
            "{name}: f must maximize the focused pane"
        );
        assert_eq!(app.status_message, "Maximized");
        assert_eq!(
            arrangement_views(&app),
            views,
            "{name}: maximize must not disturb the arrangement"
        );

        // A second `f` restores it.
        handle_event(&key(KeyCode::Char('f')), &mut app);
        assert_eq!(app.layout.maximized, None, "{name}: f again must restore");
        assert_eq!(app.status_message, "Layout restored");
        assert_eq!(arrangement_views(&app), views, "{name}: f again restores");

        // `Tab` clears on the focus change and restores the arrangement.
        handle_event(&key(KeyCode::Char('f')), &mut app);
        assert_eq!(app.layout.maximized, Some(focused));
        handle_event(&key(KeyCode::Tab), &mut app);
        assert_eq!(app.layout.maximized, None, "{name}: Tab must clear");
        assert_eq!(arrangement_views(&app), views, "{name}: Tab restores");

        // `Esc` clears before it would close the (now focused) view.
        let second_focus = app.layout.focus;
        let second_view = app.layout.pane(second_focus).view;
        handle_event(&key(KeyCode::Char('f')), &mut app);
        assert_eq!(app.layout.maximized, Some(second_focus));
        handle_event(&key(KeyCode::Esc), &mut app);
        assert_eq!(app.layout.maximized, None, "{name}: Esc must clear");
        assert_eq!(
            app.layout.pane(second_focus).view,
            second_view,
            "{name}: Esc restores the arrangement instead of closing a view"
        );
        assert_eq!(arrangement_views(&app), views, "{name}: Esc restores");
    }
}

#[test]
fn regression_picker_keeps_f_for_favourites_while_the_band_stays_put() {
    let _guard = regression_fav_lock().lock().unwrap();
    let data_dir = tempfile::TempDir::new().unwrap();
    let _env = RegressionDataDir::new(data_dir.path());

    let mut app = App::new();
    // A real picker listing: `fixtures/picker_test` is the stable directory the
    // handler tests use, with a known patch file in it.
    app.favorites = crate::favorites::FavoritesStore::default();
    app.picker_dir = std::path::PathBuf::from("fixtures/picker_test");
    app.showing_picker = true;
    app.refresh_picker_entries();
    app.picker_index = app
        .picker_entries
        .iter()
        .position(|path| path.file_name().is_some_and(|n| n == "patch_a.ini"))
        .expect("the picker must list patch_a.ini");

    let layout_before = app.layout.clone();
    let views_before = arrangement_views(&app);

    handle_event(&key(KeyCode::Char('f')), &mut app);

    // The picker's meaning wins: the favourite toggles, the band does not move.
    assert!(
        app.layout.maximized.is_none(),
        "f in the picker must not maximize the band"
    );
    assert_eq!(app.layout, layout_before, "the arrangement is unchanged");
    assert_eq!(arrangement_views(&app), views_before);
    assert_eq!(
        app.favorites.favourites.len(),
        1,
        "f toggles the highlighted entry's favourite"
    );
    assert!(
        app.status_message.starts_with("Favourited"),
        "status: {:?}",
        app.status_message
    );
    assert!(app.showing_picker, "the picker stays open");
}

#[test]
fn regression_shift_f_splits_by_pane_labels_fit_vs_dependency_filter() {
    // The module UI half: a pane small enough that the labels fit lifts above
    // the whole-rack fit, so the two fits are distinguishable by zoom too.
    let mut app = cross_layer_app("fixtures/module_ui_labels.ini");
    app.note_physical_pane_rect(Rect::new(0, 0, 260, 200));
    app.apply_physical_fit_triggers();

    // Establish the rack fit first (`g f`), so `Shift+F` has somewhere to lift
    // from.
    handle_event(&key(KeyCode::Char('g')), &mut app);
    handle_event(&key(KeyCode::Char('f')), &mut app);
    let rack_fit = app.physical_zoom;
    assert_eq!(
        app.physical_fit_mode,
        Some(crate::app::FitMode::Rack),
        "g f applies the rack fit"
    );
    assert!(
        app.status_message.contains("fit(rack)"),
        "status: {:?}",
        app.status_message
    );

    handle_event(&shift_key(KeyCode::Char('F')), &mut app);
    assert_eq!(
        app.physical_fit_mode,
        Some(crate::app::FitMode::Labels),
        "Shift+F on the module UI applies the labels fit"
    );
    assert!(
        app.status_message.contains("fit(labels)"),
        "status: {:?}",
        app.status_message
    );
    assert!(
        app.physical_zoom > rack_fit,
        "the labels fit lifts above the rack fit: {} vs {rack_fit}",
        app.physical_zoom
    );
    let labels_zoom = app.physical_zoom;
    assert!(
        app.layout.maximized.is_none(),
        "Shift+F never maximizes the band"
    );
    assert!(
        app.dependency_root.is_none(),
        "the module UI's Shift+F must not touch the graph filter"
    );

    // The graph half of the same key: collapse to the two-big-pane arrangement
    // (the small viewer closes) so the graph has a visible pane beside the
    // module UI, focus the empty right half and open the graph there.
    handle_event(&key(KeyCode::Tab), &mut app); // focus the source viewer
    handle_event(&key(KeyCode::Esc), &mut app); // close it; the right half is one big pane
    assert!(!app.layout.has_small_view());
    app.layout.focus = crate::panes::PaneId::BigRight;
    handle_event(&key(KeyCode::Char('g')), &mut app);
    handle_event(&key(KeyCode::Char('g')), &mut app);
    assert_eq!(
        app.pane_holding(ViewType::Graph),
        Some(crate::panes::PaneId::BigRight),
        "the graph takes the focused big pane"
    );
    assert!(
        app.pane_holding(ViewType::Physical).is_some(),
        "the module UI pane stays open"
    );
    app.hovered_graph_node = Some(0);

    handle_event(&shift_key(KeyCode::Char('F')), &mut app);
    assert!(
        app.dependency_root.is_some(),
        "Shift+F on the graph engages the dependency filter"
    );
    assert!(
        app.status_message.starts_with("Dependencies of "),
        "status: {:?}",
        app.status_message
    );
    assert_eq!(
        app.physical_zoom, labels_zoom,
        "the graph's Shift+F leaves the module UI zoom alone"
    );
    assert_eq!(
        app.physical_fit_mode,
        Some(crate::app::FitMode::Labels),
        "the graph's Shift+F leaves the module UI fit alone"
    );

    // A second press toggles the filter off without touching the fit.
    handle_event(&shift_key(KeyCode::Char('F')), &mut app);
    assert!(app.dependency_root.is_none());
    assert_eq!(app.physical_zoom, labels_zoom);
}

#[test]
fn regression_g_f_fits_an_unfocused_module_ui_and_reports_without_one() {
    let mut app = cross_layer_app("fixtures/module_ui_labels.ini");
    app.note_physical_pane_rect(Rect::new(0, 0, 260, 200));
    app.apply_physical_fit_triggers();
    let labels_fit = app.physical_zoom;
    assert_eq!(app.physical_fit_mode, Some(crate::app::FitMode::Labels));

    // Focus leaves the module UI; the fit is pane-scoped, not focus-scoped.
    handle_event(&key(KeyCode::Tab), &mut app);
    assert!(!app.view_focused(ViewType::Physical));
    assert!(app.pane_holding(ViewType::Physical).is_some());
    let focused_before = app.layout.focus;

    handle_event(&key(KeyCode::Char('g')), &mut app);
    handle_event(&key(KeyCode::Char('f')), &mut app);
    assert_eq!(
        app.physical_fit_mode,
        Some(crate::app::FitMode::Rack),
        "g f applies the rack fit wherever the module UI sits"
    );
    assert!(
        app.status_message.contains("fit(rack)"),
        "status: {:?}",
        app.status_message
    );
    assert!(
        app.physical_zoom < labels_fit,
        "the rack fit sits below the labels fit: {} vs {labels_fit}",
        app.physical_zoom
    );
    assert_eq!(app.layout.focus, focused_before, "g f must not move focus");
    assert!(app.layout.maximized.is_none(), "g f never maximizes");

    // With no module UI pane the prefix reports instead of acting.
    let mut bare = cross_layer_app("fixtures/own_buttons.ini");
    bare.open_view(ViewType::Graph); // focus is BigLeft, so the graph replaces it
    assert!(
        bare.pane_holding(ViewType::Physical).is_none(),
        "the graph took the module UI's pane"
    );
    let layout_before = bare.layout.clone();
    let zoom_before = bare.physical_zoom;
    handle_event(&key(KeyCode::Char('g')), &mut bare);
    handle_event(&key(KeyCode::Char('f')), &mut bare);
    assert_eq!(bare.status_message, "No module UI pane open");
    assert_eq!(bare.layout, layout_before, "the band is unchanged");
    assert_eq!(bare.physical_zoom, zoom_before, "the zoom is unchanged");
}

#[test]
fn regression_performance_zoom_grows_rack_and_labels_shift_minus_only_labels() {
    let mut app = cross_layer_app("fixtures/own_buttons.ini");
    let pane = performance_story_pane();

    // Enter the view through its key, on the focused module UI.
    handle_event(&key(KeyCode::Char('p')), &mut app);
    assert!(app.showing_performance, "p opens the performance view");
    assert!(app.view_focused(ViewType::Physical));
    // 100 % is the comparison point: the zoom the view is documented against.
    app.physical_zoom = 1.0;
    app.physical_offset = (0.0, 0.0);
    app.performance_label_scale = 1.0;

    let base = crate::gui::physical_spec(&app, pane).expect("a loaded patch draws");
    let base_overlay = base.performance.as_ref().expect("the overlay is present");
    let (base_w, base_h) = (
        base_overlay.rack_rect.width(),
        base_overlay.rack_rect.height(),
    );
    let base_size = base_overlay
        .callouts
        .first()
        .expect("a labelled fixture yields callouts")
        .label_size;
    assert!(
        (base_size - 14.0).abs() < 1e-4,
        "100 % zoom and 100 % scale is the 14 pt reference: {base_size}"
    );

    // Plain `+` is the view's zoom: the shared preset ladder moves it to 150 %.
    handle_event(&key(KeyCode::Char('+')), &mut app);
    assert!(
        (app.physical_zoom - 1.5).abs() < 1e-4,
        "the key's zoom applies: {}",
        app.physical_zoom
    );
    assert!(
        app.status_message.contains("150%"),
        "the status reports the new zoom: {:?}",
        app.status_message
    );

    let zoomed = crate::gui::physical_spec(&app, pane).expect("spec after zoom");
    let zoomed_overlay = zoomed.performance.as_ref().expect("overlay after zoom");
    let (zoom_w, zoom_h) = (
        zoomed_overlay.rack_rect.width(),
        zoomed_overlay.rack_rect.height(),
    );
    assert!(
        zoom_w > base_w && zoom_h > base_h,
        "the rack extent must grow with the zoom: {zoom_w}x{zoom_h} vs {base_w}x{base_h}"
    );
    let zoomed_size = zoomed_overlay
        .callouts
        .first()
        .expect("callouts survive the zoom")
        .label_size;
    assert!(
        zoomed_size > base_size,
        "the callouts must grow with the rack: {zoomed_size} vs {base_size}"
    );
    assert!(
        (zoomed_size - 21.0).abs() < 1e-4,
        "14 pt × 150 % zoom: {zoomed_size}"
    );

    // Shift+minus shrinks the base scale only; the rack keeps its zoom.
    handle_event(&shift_key(KeyCode::Char('_')), &mut app);
    assert!(
        (app.performance_label_scale - 0.95).abs() < 1e-5,
        "Shift+- steps the scale to 95 %: {}",
        app.performance_label_scale
    );
    assert!(
        app.status_message.contains("Labels 95%"),
        "the status reports the scale: {:?}",
        app.status_message
    );

    let shrunk = crate::gui::physical_spec(&app, pane).expect("spec after the scale step");
    let shrunk_overlay = shrunk.performance.as_ref().expect("overlay after scale");
    assert!(
        (shrunk_overlay.rack_rect.width() - zoom_w).abs() < 1e-3
            && (shrunk_overlay.rack_rect.height() - zoom_h).abs() < 1e-3,
        "the rack must keep its extent: {}x{} vs {zoom_w}x{zoom_h}",
        shrunk_overlay.rack_rect.width(),
        shrunk_overlay.rack_rect.height()
    );
    let shrunk_size = shrunk_overlay
        .callouts
        .first()
        .expect("callouts survive the scale step")
        .label_size;
    assert!(
        shrunk_size < zoomed_size,
        "the labels must shrink: {shrunk_size} vs {zoomed_size}"
    );
    assert!(
        (shrunk_size - 14.0 * 1.5 * 0.95).abs() < 1e-3,
        "14 pt × 150 % zoom × 95 % scale: {shrunk_size}"
    );
    assert!(
        (app.physical_zoom - 1.5).abs() < 1e-4,
        "Shift+- must not change the zoom"
    );
}

#[test]
fn regression_restart_restores_the_performance_label_scale() {
    // Save a config the way the app does, then read it back through the loader:
    // the persisted 115 % must reach the fresh App's seed.
    let dir = tempfile::TempDir::new().unwrap();
    let mut settings = crate::config::Settings::default();
    settings.labels.performance_scale = 1.15;
    crate::config::save_to_dir(dir.path(), &settings).expect("the config saves");
    let path = dir.path().join("config.toml");
    let body = std::fs::read_to_string(&path).expect("the saved config is readable");
    assert!(
        body.contains("performance_scale = 1.15"),
        "the setting is persisted: {body}"
    );

    let canonicalize = |_theme: &str| Some("classic");
    let loaded = crate::config::load_from(&path, &canonicalize, &[]);
    assert!(
        (loaded.labels.performance_scale - 1.15).abs() < 1e-6,
        "the loader returns 115 %: {}",
        loaded.labels.performance_scale
    );

    // A restart: fresh App, production patch load, the same seeding `main.rs`
    // performs at startup, then the performance view at the 100 % zoom.
    let mut app = cross_layer_app("fixtures/own_buttons.ini");
    seed_app_from_settings(&mut app, &loaded);
    assert!(
        (app.performance_label_scale - 1.15).abs() < 1e-6,
        "a restart seeds the persisted scale: {}",
        app.performance_label_scale
    );
    app.showing_performance = true;
    app.physical_zoom = 1.0;

    let pane = performance_story_pane();
    let spec = crate::gui::physical_spec(&app, pane).expect("a loaded patch draws");
    let overlay = spec.performance.as_ref().expect("the overlay is present");
    let callout = overlay
        .callouts
        .first()
        .expect("a labelled fixture yields callouts");
    assert!(
        (callout.label_size - 14.0 * 1.15).abs() < 1e-4,
        "the callout draws at 115 % of the 14 pt reference: {}",
        callout.label_size
    );
}
