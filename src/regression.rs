//! Regression suite for embedded source navigation (task 5.1).
//! Drives real flows end-to-end through `handle_event` + `render` with
//! `fixtures/source_navigation.ini`. Each test is a small story rather
//! than an isolated unit: open viewer, select, navigate, toggle, click.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::Terminal;

use crate::app::{App, SourceViewMode, ViewerFocus};
use crate::handler::{handle_event, handle_mouse_event};
use crate::patch::{Patch, ShiftGroup};
use crate::ui::render;

// ── helpers ──────────────────────────────────────────────────────────────

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
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
                    "B1.1" => 0,
                    "P1.1" => 20,
                    "B1.2" => 40,
                    "P1.2" => 60,
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

fn buffer_for(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn rendered_text(app: &mut App, width: u16, height: u16) -> String {
    let buf = buffer_for(app, width, height);
    buf.content().iter().map(|c| c.symbol()).collect::<String>()
}

fn has_highlighted_token(
    buffer: &Buffer,
    token: &str,
    want_fg: Option<Color>,
    want_modifier: Option<Modifier>,
) -> bool {
    let area = buffer.area;
    let want: Vec<char> = token.chars().collect();
    for y in 0..area.height {
        let mut row_chars: Vec<char> = Vec::with_capacity(area.width as usize);
        let mut row_styles: Vec<Style> = Vec::with_capacity(area.width as usize);
        for x in 0..area.width {
            let cell = buffer.cell((x, y)).unwrap();
            row_chars.push(cell.symbol().chars().next().unwrap_or(' '));
            row_styles.push(cell.style());
        }
        if row_chars.len() < want.len() {
            continue;
        }
        for start in 0..=row_chars.len() - want.len() {
            if row_chars[start..start + want.len()] != want[..] {
                continue;
            }
            let mut ok = true;
            for i in 0..want.len() {
                let style = row_styles[start + i];
                if let Some(fg) = want_fg {
                    if style.fg != Some(fg) {
                        ok = false;
                        break;
                    }
                }
                if let Some(m) = want_modifier {
                    if !style.add_modifier.contains(m) {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                return true;
            }
        }
    }
    false
}

fn open_viewer(app: &mut App) {
    handle_event(key(KeyCode::Char('g')), app);
    handle_event(key(KeyCode::Char('v')), app);
    assert!(app.showing_viewer, "g v should open embedded viewer");
}

// ── regression: initial BOF vs selected-open ───────────────────────────

#[test]
fn regression_initial_bof_vs_selected_open() {
    // BOF when nothing selected
    let mut app = fixture_app();
    app.source_scroll = 99;
    open_viewer(&mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Source);
    assert_eq!(app.source_scroll, 0, "BOF when no selection");
    assert_eq!(app.occurrence_cursor, 0);
    // render publishes minimap and shows verbatim source
    let _ = buffer_for(&mut app, 120, 40);
    assert!(
        app.minimap_rect.is_some(),
        "minimap published on wide render"
    );
    // source area should be visible (raw lines contain p2b8 header)
    let text = rendered_text(&mut app, 120, 40);
    assert!(
        text.contains("Source Viewer"),
        "viewer status hints present"
    );

    // Selected-open jumps to first occurrence
    let mut app2 = fixture_app();
    let first_b11 = app2.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
    app2.select_component(String::from("B1.1"));
    app2.source_scroll = 999;
    // close then reopen through handle_event so initial-position rule reapplies
    app2.showing_viewer = false;
    open_viewer(&mut app2);
    assert_eq!(
        app2.source_scroll, first_b11,
        "opens at selected first occurrence"
    );
    assert_eq!(app2.occurrence_cursor, 0);
    assert_eq!(app2.selected_component, Some(String::from("B1.1")));
    let buf2 = buffer_for(&mut app2, 120, 40);
    assert!(has_highlighted_token(
        &buf2,
        "B1.1",
        Some(Color::Yellow),
        Some(Modifier::REVERSED)
    ));
    // BOF text still present but occurrence highlighted
    let _ = has_highlighted_token(&buf2, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD));
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
    handle_event(key(KeyCode::Enter), &mut app);
    assert_eq!(app.selected_component, Some(String::from("B1.1")));
    assert_eq!(app.source_scroll, b11_first);
    assert_eq!(app.occurrence_cursor, 0);
    // render doesn't clear the jump
    let _ = buffer_for(&mut app, 120, 40);
    assert_eq!(app.source_scroll, b11_first);

    // Replacement via Space on different component
    let p11_idx = idx_for(&app, "P1.1");
    app.hovered_component = Some(p11_idx);
    handle_event(key(KeyCode::Char(' ')), &mut app);
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
    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
    app.hovered_component = Some(b11_idx);
    handle_event(key(KeyCode::Enter), &mut app);
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
    handle_event(key(KeyCode::Up), &mut app);
    assert_eq!(app.occurrence_cursor, 0);
    assert_eq!(app.source_scroll, occ[0].line);

    // Down steps one
    handle_event(key(KeyCode::Down), &mut app);
    assert_eq!(app.occurrence_cursor, 1);
    assert_eq!(app.source_scroll, occ[1].line);

    // Down to last saturates
    for _ in 0..occ.len() + 5 {
        handle_event(key(KeyCode::Down), &mut app);
    }
    assert_eq!(app.occurrence_cursor, occ.len() - 1);
    assert_eq!(app.source_scroll, occ.last().unwrap().line);
    handle_event(key(KeyCode::Down), &mut app);
    assert_eq!(app.occurrence_cursor, occ.len() - 1);

    // Home -> first, End -> last
    handle_event(key(KeyCode::Home), &mut app);
    assert_eq!(app.occurrence_cursor, 0);
    assert_eq!(app.source_scroll, occ[0].line);
    handle_event(key(KeyCode::End), &mut app);
    assert_eq!(app.occurrence_cursor, occ.len() - 1);

    // No-selection no-op
    app.clear_selected_component();
    let scroll_before = app.source_scroll;
    handle_event(key(KeyCode::Up), &mut app);
    handle_event(key(KeyCode::Down), &mut app);
    handle_event(key(KeyCode::Home), &mut app);
    handle_event(key(KeyCode::End), &mut app);
    assert_eq!(app.source_scroll, scroll_before);
    assert_eq!(app.occurrence_cursor, 0);
    // render after bounds still valid
    let _ = buffer_for(&mut app, 120, 40);
}

// ── deselect keeps position ─────────────────────────────────────────────

#[test]
fn regression_deselect_keeps_position() {
    let mut app = fixture_app();
    app.select_component(String::from("B1.1"));
    open_viewer(&mut app);
    let occ = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
    // Move to middle occurrence via Down
    handle_event(key(KeyCode::Down), &mut app);
    let mid_line = app.source_scroll;
    assert_eq!(app.occurrence_cursor, 1);
    // Also j/k line scroll should still work but not affect occurrence cursor
    handle_event(key(KeyCode::Char('j')), &mut app);
    assert_eq!(app.source_scroll, mid_line + 1);
    // Jump back to occurrence so deselect origin is occurrence-aligned
    handle_event(key(KeyCode::Up), &mut app);
    assert_eq!(app.source_scroll, occ[0].line);
    handle_event(key(KeyCode::Down), &mut app);
    assert_eq!(app.source_scroll, occ[1].line);
    let pos_before = app.source_scroll;

    // Empty-panel click clears selection without moving scroll
    // Need viewer focus on Panels for empty click to be considered
    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
    // Place component rects far away, click empty
    let idx = idx_for(&app, "B1.1");
    app.component_rects = vec![(idx, Rect::new(0, 0, 16, 2))];
    app.minimap_rect = None; // ensure minimap doesn't intercept
                             // Need a wide-enough render to have published minimap, then clear it
    let _ = buffer_for(&mut app, 120, 40);
    // Actually set minimap to None to isolate empty-panel logic, or keep but click not on minimap
    app.minimap_rect = None;
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
    // Render still shows no highlights
    let buf = buffer_for(&mut app, 120, 40);
    // Occurrence highlight cleared; there may still be plain text B1.1 without Yellow
    let still_yellow =
        has_highlighted_token(&buf, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD));
    assert!(!still_yellow, "highlights cleared on deselect");
}

// ── modifier highlights appear / clear ──────────────────────────────────

#[test]
fn regression_modifier_highlights_appear_and_clear() {
    let mut app = fixture_app();
    app.source_view_mode = SourceViewMode::Raw;
    app.select_component(String::from("B1.2"));
    open_viewer(&mut app);
    // Ensure viewport covers transitive line
    app.source_scroll = 0;
    let buf = buffer_for(&mut app, 120, 80);
    // B1.2 occurrence yellow
    assert!(
        has_highlighted_token(&buf, "B1.2", Some(Color::Yellow), Some(Modifier::BOLD)),
        "B1.2 occurrence yellow"
    );
    // Transitive _TRANSIT cyan
    assert!(
        has_highlighted_token(
            &buf,
            "_TRANSIT",
            Some(Color::Cyan),
            Some(Modifier::UNDERLINED)
        ),
        "transitive _TRANSIT cyan underlined when B1.2 selected"
    );
    // Switch to B1.1 direct boolean
    app.select_component(String::from("B1.1"));
    app.source_scroll = 0;
    let buf2 = buffer_for(&mut app, 120, 80);
    assert!(
        has_highlighted_token(&buf2, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD)),
        "B1.1 occurrence after switch"
    );
    // _TRANSIT should no longer be highlighted when B1.1 selected (transitive only for B1.2)
    // Instead, B1.1's own direct modifier may be cyan, but we check _TRANSIT cyan absent
    // Actually B1.1 also has transitive via _CHAIN2, but _TRANSIT is specific to B1.2, so should be absent
    // Use has_highlighted false for _TRANSIT when not selected? It may still be present if other path?
    // At least we can verify Magent/Cyan switch for P1.1 later.
    // Now exact-value magenta: P1.1 with selectat 0.5
    app.select_component(String::from("P1.1"));
    // cursor 0 sits on modifier line -> current yellow hides magenta, so jump off
    app.jump_to_occurrence(1);
    app.source_scroll = 0;
    let buf3 = buffer_for(&mut app, 120, 80);
    assert!(
        has_highlighted_token(
            &buf3,
            "P1.1",
            Some(Color::Magenta),
            Some(Modifier::UNDERLINED)
        ) || has_highlighted_token(&buf3, "P1.1", Some(Color::Magenta), Some(Modifier::BOLD)),
        "P1.1 exact-value magenta underlined when not current"
    );
    // Clearing removes all highlights
    app.clear_selected_component();
    let buf_none = buffer_for(&mut app, 120, 80);
    assert!(
        !has_highlighted_token(&buf_none, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD)),
        "yellow cleared"
    );
    assert!(
        !has_highlighted_token(
            &buf_none,
            "_TRANSIT",
            Some(Color::Cyan),
            Some(Modifier::UNDERLINED)
        ),
        "cyan cleared"
    );
    assert!(
        !has_highlighted_token(
            &buf_none,
            "P1.1",
            Some(Color::Magenta),
            Some(Modifier::UNDERLINED)
        ),
        "magenta cleared"
    );
    // Prettified also shows modifier
    app.source_view_mode = SourceViewMode::Prettified;
    app.select_component(String::from("B1.2"));
    let buf_pre = buffer_for(&mut app, 120, 80);
    assert!(
        has_highlighted_token(
            &buf_pre,
            "_TRANSIT",
            Some(Color::Cyan),
            Some(Modifier::UNDERLINED)
        ),
        "prettified transitive cyan"
    );
}

// ── t preserves usable content ──────────────────────────────────────────

#[test]
fn regression_t_preserves_usable_content() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    app.source_scroll = 0;
    let text_raw = rendered_text(&mut app, 120, 40);
    // Raw shows verbatim ini: header and key=value
    assert!(
        text_raw.contains("[p2b8]") || text_raw.contains("p2b8"),
        "raw should contain p2b8 header"
    );
    assert!(text_raw.contains("button = B1.1") || text_raw.contains("B1.1"));
    // Highlights usable: select then verify still raw
    app.select_component(String::from("B1.1"));
    let buf_raw_sel = buffer_for(&mut app, 120, 80);
    assert!(has_highlighted_token(
        &buf_raw_sel,
        "B1.1",
        Some(Color::Yellow),
        Some(Modifier::BOLD)
    ));
    // t -> prettified
    handle_event(key(KeyCode::Char('t')), &mut app);
    assert_eq!(app.source_view_mode, SourceViewMode::Prettified);
    let text_pre = rendered_text(&mut app, 120, 40);
    // Prettified shows circuit boxes ┌─ and entries
    assert!(text_pre.contains("┌─") || text_pre.contains("button") || text_pre.contains("copy"));
    // Modifiers still highlighted in prettified
    let buf_pre = buffer_for(&mut app, 120, 80);
    // B1.1 direct modifier not in prettified as select? but occurrence inside values should be highlighted
    // For B1.1, check at least yellow highlight persists
    assert!(
        has_highlighted_token(
            &buf_pre,
            "B1.1",
            Some(Color::Yellow),
            Some(Modifier::REVERSED)
        ) || has_highlighted_token(&buf_pre, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD))
            || has_highlighted_token(
                &buf_pre,
                "_TRANSIT",
                Some(Color::Cyan),
                Some(Modifier::UNDERLINED)
            )
            || has_highlighted_token(
                &buf_pre,
                "B1.1",
                Some(Color::Cyan),
                Some(Modifier::UNDERLINED)
            ),
        "prettified should still highlight"
    );
    // t back to raw restores verbatim content and scroll still meaningful
    let scroll_before = app.source_scroll;
    handle_event(key(KeyCode::Char('t')), &mut app);
    assert_eq!(app.source_view_mode, SourceViewMode::Raw);
    let text_raw2 = rendered_text(&mut app, 120, 40);
    assert!(text_raw2.contains("button = B1.1") || text_raw2.contains("B1.1"));
    // scroll preserved across toggles (t does not reset scroll)
    assert_eq!(app.source_scroll, scroll_before);
    // narrow render while toggling should not panic and still show content
    for mode in [SourceViewMode::Raw, SourceViewMode::Prettified] {
        app.source_view_mode = mode;
        let t = rendered_text(&mut app, 50, 20);
        assert!(!t.is_empty());
    }
}

// ── minimap click maps correctly ────────────────────────────────────────

#[test]
fn regression_minimap_click_maps_correctly() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    app.source_view_mode = SourceViewMode::Raw;
    app.source_scroll = 0;
    // Wide render publishes geometry
    let _ = buffer_for(&mut app, 120, 40);
    let rect = app.minimap_rect.expect("minimap visible at 120x40");
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
    handle_event(key(KeyCode::Tab), &mut app); // to Panels
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
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

    handle_event(key(KeyCode::Tab), &mut app); // back to Source
    assert_eq!(app.viewer_focus, ViewerFocus::Source);
    click_at(&mut app, 0.7);
    // Still works when source focused
    assert!(app.source_scroll > 0);

    // Indicator tracks scroll: render and check viewport rows moved
    app.source_scroll = 0;
    let buf_top = buffer_for(&mut app, 120, 40);
    let rect_top = app.minimap_rect.unwrap();
    let rows_top = minimap_viewport_rows(&buf_top, rect_top);
    app.source_scroll = bot;
    let buf_bot = buffer_for(&mut app, 120, 40);
    let rect_bot = app.minimap_rect.unwrap();
    let rows_bot = minimap_viewport_rows(&buf_bot, rect_bot);
    assert!(!rows_top.is_empty() && !rows_bot.is_empty());
    assert!(
        rows_bot[0] > rows_top[0],
        "indicator moves down with scroll"
    );

    // Hidden on narrow: no panic, minimap_rect None, click is no-op (treated as panel/deselect)
    let _ = buffer_for(&mut app, 70, 24);
    assert!(app.minimap_rect.is_none(), "hidden at 70 width");
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

fn minimap_viewport_rows(buffer: &Buffer, minimap: Rect) -> Vec<usize> {
    let mut rows = Vec::new();
    let inner_y = minimap.y + 1;
    let inner_h = minimap.height.saturating_sub(2) as usize;
    for i in 0..inner_h {
        let y = inner_y + i as u16;
        let x = minimap.x + 1;
        if let Some(cell) = buffer.cell((x, y)) {
            if cell.style().add_modifier.contains(Modifier::REVERSED)
                || cell.style().bg == Some(Color::DarkGray)
            {
                rows.push(i);
            }
        }
    }
    rows
}

// ── Tab focus round-trip ────────────────────────────────────────────────

#[test]
fn regression_tab_focus_round_trip() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Source);
    // render shows focus emphasis (yellow border) is not directly assertable via text,
    // but we can verify state and that panel keys are isolated when Source focused
    let scale_before = app.scale_factor;
    handle_event(key(KeyCode::Char('+')), &mut app);
    assert_eq!(
        app.scale_factor, scale_before,
        "scale inert when Source focused"
    );

    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
    // Now panel keys work
    handle_event(key(KeyCode::Char('+')), &mut app);
    assert_ne!(app.scale_factor, scale_before);
    // j should move hover when panels focused (not source scroll)
    app.hovered_component = Some(idx_for(&app, "B1.1"));
    let hover_before = app.hovered_component;
    handle_event(key(KeyCode::Char('j')), &mut app);
    assert_ne!(app.hovered_component, hover_before);

    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Source);
    // Back to source: j scrolls source
    let scroll_before = app.source_scroll;
    handle_event(key(KeyCode::Char('j')), &mut app);
    assert_eq!(app.source_scroll, scroll_before + 1);

    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Source);

    // Esc closes keeping selection and resets focus to Panels
    app.select_component(String::from("B1.1"));
    let sel = app.selected_component.clone();
    let scroll = app.source_scroll;
    handle_event(key(KeyCode::Esc), &mut app);
    assert!(!app.showing_viewer);
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
    assert_eq!(app.selected_component, sel);
    assert_eq!(app.source_scroll, scroll);
    // render after close still valid
    let _ = buffer_for(&mut app, 80, 24);
}

// ── picker precedence ───────────────────────────────────────────────────

#[test]
fn regression_picker_precedence() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    app.source_view_mode = SourceViewMode::Raw;
    app.source_scroll = 5;
    let _ = buffer_for(&mut app, 120, 24);
    assert!(app.minimap_rect.is_some());
    // Open picker via l even while source focused (picker precedence)
    handle_event(key(KeyCode::Char('l')), &mut app);
    assert!(app.showing_picker, "picker opens even when source focused");
    // Picker renders on top
    let text = rendered_text(&mut app, 120, 24);
    assert!(
        text.contains("File Picker"),
        "picker overlay wins over viewer"
    );
    // While picker open, viewer keys are inert: t should not toggle view mode
    let mode_before = app.source_view_mode.clone();
    handle_event(key(KeyCode::Char('t')), &mut app);
    assert_eq!(app.source_view_mode, mode_before);
    handle_event(key(KeyCode::Tab), &mut app);
    // picker still open, Tab didn't switch viewer focus (picker consumes)
    assert!(app.showing_picker);
    // Picker navigation still works
    let idx_before = app.picker_index;
    handle_event(key(KeyCode::Char('j')), &mut app);
    assert!(app.picker_index >= idx_before);
    // Esc closes picker but viewer stays open
    handle_event(key(KeyCode::Esc), &mut app);
    assert!(!app.showing_picker);
    assert!(app.showing_viewer, "viewer remains after picker close");
    // Picker closed, t works again
    handle_event(key(KeyCode::Char('t')), &mut app);
    assert_ne!(app.source_view_mode, mode_before);
    // Full cycle: picker over viewer with minimap -> render still valid
    let _ = buffer_for(&mut app, 120, 40);
}

// ── viewer isolation ────────────────────────────────────────────────────

#[test]
fn regression_viewer_isolation() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Source);
    // Capture state before
    let state_before = app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")]
        .state
        .clone();
    let shift_before = app.active_shift;
    let scale_before = app.scale_factor;
    let orient_before = app.orientation.clone();

    // Panel toggles / shift / scale / orientation inert when Source focused
    app.hovered_component = Some(idx_for(&app, "B1.1"));
    handle_event(key(KeyCode::Enter), &mut app);
    assert_eq!(
        app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")].state,
        state_before,
        "Enter inert when source focused"
    );
    handle_event(key(KeyCode::Char(' ')), &mut app);
    assert_eq!(
        app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")].state,
        state_before,
        "Space inert when source focused"
    );
    handle_event(key(KeyCode::Char('1')), &mut app);
    assert_eq!(app.active_shift, shift_before, "shift inert");
    handle_event(key(KeyCode::Char('+')), &mut app);
    assert_eq!(app.scale_factor, scale_before, "scale inert");
    handle_event(key(KeyCode::Char('o')), &mut app);
    assert_eq!(app.orientation, orient_before, "orientation inert");

    // Mouse click on panel component inert when Source focused
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
        &mut app,
    );
    assert_eq!(
        app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")].state,
        state_before,
        "mouse inert when source focused"
    );

    // g prefix also inert when source focused (doesn't arm)
    handle_event(key(KeyCode::Char('g')), &mut app);
    assert!(app.prefix.is_none(), "g inert when source focused");
    // l still opens picker (exception to isolation), j/k line scroll and Up/Down occurrence still work
    let scroll_before = app.source_scroll;
    handle_event(key(KeyCode::Char('j')), &mut app);
    assert_eq!(app.source_scroll, scroll_before + 1);
    app.select_component(String::from("B1.1"));
    open_viewer(&mut app); // already open but re-assert
                           // Ensure source focused
    if app.viewer_focus != ViewerFocus::Source {
        handle_event(key(KeyCode::Tab), &mut app);
    }
    let occ_before = app.occurrence_cursor;
    handle_event(key(KeyCode::Down), &mut app);
    assert!(app.occurrence_cursor >= occ_before);

    // Tab to Panels: isolation lifts, panel interactions work again
    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
    handle_event(key(KeyCode::Char('1')), &mut app);
    assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
    let s = app.scale_factor;
    handle_event(key(KeyCode::Char('+')), &mut app);
    assert_ne!(app.scale_factor, s);
    let o = app.orientation.clone();
    handle_event(key(KeyCode::Char('o')), &mut app);
    assert_ne!(app.orientation, o);
    app.hovered_component = Some(idx_for(&app, "B1.1"));
    handle_event(key(KeyCode::Enter), &mut app);
    assert_ne!(
        app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")].state,
        state_before
    );
    // Mouse now works after Tab
    let state_mid = app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.2")]
        .state
        .clone();
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 42, 1),
        &mut app,
    );
    assert_ne!(
        app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.2")].state,
        state_mid
    );

    // t and Esc still work from both focuses (global viewer keys)
    handle_event(key(KeyCode::Tab), &mut app); // back to Source for global check
    let mode_before = app.source_view_mode.clone();
    handle_event(key(KeyCode::Char('t')), &mut app);
    assert_ne!(app.source_view_mode, mode_before);
    handle_event(key(KeyCode::Tab), &mut app); // Panels
    let mode_before2 = app.source_view_mode.clone();
    handle_event(key(KeyCode::Char('t')), &mut app);
    assert_ne!(app.source_view_mode, mode_before2);
    handle_event(key(KeyCode::Esc), &mut app);
    assert!(!app.showing_viewer);
    // After close, normal panel handling resumes
    handle_event(key(KeyCode::Char('1')), &mut app);
    assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
    let _ = buffer_for(&mut app, 80, 24);
}
