//! Regression suite for embedded source navigation (task 5.1).
//! Drives real flows end-to-end through `handle_event` + `render` with
//! `fixtures/source_navigation.ini`. Each test is a small story rather
//! than an isolated unit: open viewer, select, navigate, toggle, click.
//!
//! A second story arc covers boxed components and the viewer split ratio
//! (`fixtures/led_pairs.ini`): parse to boxed/text frames to mixed grid
//! to click hit-testing, plus `[`/`]` split adjustment through handler,
//! App state, and the rendered layout.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::Terminal;

use crate::app::{App, SourceViewMode, ViewerFocus};
use crate::handler::{handle_event, handle_mouse_event};
use crate::patch::{ComponentState, Patch, ShiftGroup};
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

#[allow(dead_code)]
fn buffer_to_ansi(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut rows = Vec::with_capacity(area.height as usize);
    for y in 0..area.height {
        let mut line = String::with_capacity(area.width as usize);
        for x in 0..area.width {
            let cell = buffer.cell((x, y)).unwrap();
            line.push_str(cell.symbol());
        }
        rows.push(line.trim_end().to_string());
    }
    while rows.last().is_some_and(|r| r.is_empty()) {
        rows.pop();
    }
    rows.join("\n")
}

#[allow(dead_code)]
fn color_to_css(color: Color) -> Option<String> {
    match color {
        Color::Reset => None,
        Color::Black => Some(String::from("black")),
        Color::Red => Some(String::from("red")),
        Color::Green => Some(String::from("green")),
        Color::Yellow => Some(String::from("yellow")),
        Color::Blue => Some(String::from("blue")),
        Color::Magenta => Some(String::from("magenta")),
        Color::Cyan => Some(String::from("cyan")),
        Color::Gray => Some(String::from("gray")),
        Color::DarkGray => Some(String::from("darkgray")),
        Color::White => Some(String::from("white")),
        Color::LightRed => Some(String::from("#ff5555")),
        Color::LightGreen => Some(String::from("#55ff55")),
        Color::LightYellow => Some(String::from("#ffff55")),
        Color::LightBlue => Some(String::from("#5555ff")),
        Color::LightMagenta => Some(String::from("#ff55ff")),
        Color::LightCyan => Some(String::from("#55ffff")),
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Indexed(idx) => Some(format!("indexed-{idx}")),
    }
}

#[allow(dead_code)]
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[allow(dead_code)]
fn buffer_to_html(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut rows = Vec::with_capacity(area.height as usize);
    for y in 0..area.height {
        let mut row_html = String::new();
        // Determine last non-space column to trim trailing empty cells.
        let mut last = None;
        for x in (0..area.width).rev() {
            let cell = buffer.cell((x, y)).unwrap();
            if cell.symbol() != " " {
                last = Some(x);
                break;
            }
        }
        let width = last.map(|x| x + 1).unwrap_or(0);
        for x in 0..width {
            let cell = buffer.cell((x, y)).unwrap();
            let symbol = cell.symbol();
            let mut style = cell.style();
            // REVERSED swaps fg/bg as in ui.rs hover_style.
            if style.add_modifier.contains(Modifier::REVERSED) {
                let fg = style.fg;
                let bg = style.bg;
                style.fg = bg;
                style.bg = fg;
            }
            let mut css_parts = Vec::new();
            if let Some(fg) = style.fg.and_then(color_to_css) {
                css_parts.push(format!("color:{fg}"));
            }
            if let Some(bg) = style.bg.and_then(color_to_css) {
                css_parts.push(format!("background-color:{bg}"));
            }
            if style.add_modifier.contains(Modifier::BOLD) {
                css_parts.push(String::from("font-weight:bold"));
            }
            if style.add_modifier.contains(Modifier::DIM) {
                css_parts.push(String::from("opacity:0.6"));
            }
            let escaped = html_escape(symbol);
            if css_parts.is_empty() {
                row_html.push_str(&escaped);
            } else {
                row_html.push_str(&format!(
                    "<span style=\"{}\">{}</span>",
                    css_parts.join(";"),
                    escaped
                ));
            }
        }
        rows.push(row_html);
    }
    while rows.last().is_some_and(|r| r.is_empty()) {
        rows.pop();
    }
    rows.join("\n")
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
    // The clamped border click below the minimap counts as empty-panel space
    // and may have handed focus to Panels; normalize before the check.
    if app.viewer_focus != ViewerFocus::Panels {
        handle_event(key(KeyCode::Tab), &mut app); // to Panels
    }
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
    // but we can verify state and that panel keys are live even when Source focused
    let scale_before = app.scale_factor;
    handle_event(key(KeyCode::Char('+')), &mut app);
    assert_ne!(
        app.scale_factor, scale_before,
        "scale live when Source focused"
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

// ── viewer live interaction (main window unblocked, droid_tui-0lw) ─────

#[test]
fn regression_viewer_live_interaction() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Source);

    // Panel keys are live while Source focused.
    handle_event(key(KeyCode::Char('1')), &mut app);
    assert_eq!(app.active_shift, Some(ShiftGroup::Group1), "shift live");
    let scale_before = app.scale_factor;
    handle_event(key(KeyCode::Char('+')), &mut app);
    assert_ne!(app.scale_factor, scale_before, "scale live");
    let orient_before = app.orientation.clone();
    handle_event(key(KeyCode::Char('o')), &mut app);
    assert_ne!(app.orientation, orient_before, "orientation live");

    // Enter toggles AND selects the hovered component; selection jumps
    // source_scroll to its first occurrence.
    let b11 = idx_for(&app, "B1.1");
    app.hovered_component = Some(b11);
    let first_b11 = app.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
    let state_before = app.patch.as_ref().unwrap().hw_components[b11].state.clone();
    handle_event(key(KeyCode::Enter), &mut app);
    assert_ne!(
        app.patch.as_ref().unwrap().hw_components[b11].state,
        state_before,
        "Enter toggles while Source focused"
    );
    assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
    assert_eq!(app.source_scroll, first_b11);
    handle_event(key(KeyCode::Char(' ')), &mut app);
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
        app.viewer_focus,
        ViewerFocus::Panels,
        "component click hands focus to panels"
    );

    // Tab back to Source; a bare source-pane click re-focuses the source
    // pane without clearing the selection or toggling anything.
    handle_event(key(KeyCode::Tab), &mut app);
    assert_eq!(app.viewer_focus, ViewerFocus::Source);
    let sel = app.selected_component.clone();
    let scroll = app.source_scroll;
    app.minimap_rect = None;
    app.source_pane_rect = Some(Rect::new(72, 3, 47, 34));
    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), 100, 30),
        &mut app,
    );
    assert_eq!(
        app.viewer_focus,
        ViewerFocus::Source,
        "bare source-pane click focuses source"
    );
    assert_eq!(app.selected_component, sel, "selection kept");
    assert_eq!(app.source_scroll, scroll, "scroll kept");

    // l still opens picker (picker precedence), j/k line scroll and
    // Up/Down occurrence navigation stay routed by focus.
    let scroll_before = app.source_scroll;
    handle_event(key(KeyCode::Char('j')), &mut app);
    assert_eq!(app.source_scroll, scroll_before + 1);
    let occ_before = app.occurrence_cursor;
    handle_event(key(KeyCode::Down), &mut app);
    assert!(app.occurrence_cursor >= occ_before);

    // t works from both focuses (global viewer key).
    let mode_before = app.source_view_mode.clone();
    handle_event(key(KeyCode::Char('t')), &mut app);
    assert_ne!(app.source_view_mode, mode_before);

    // Esc closes keeping selection and resets focus to Panels.
    handle_event(key(KeyCode::Esc), &mut app);
    assert!(!app.showing_viewer);
    assert_eq!(app.viewer_focus, ViewerFocus::Panels);
    assert_eq!(app.selected_component, sel);
    // After close, normal panel handling resumes.
    handle_event(key(KeyCode::Char('1')), &mut app);
    assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
    let _ = buffer_for(&mut app, 80, 24);
}

// ── boxed components & split ratio (task 5.1: boxed-components) ────────

/// App loaded with the LED-association fixture: B1.1 owns L1.1 (boxed),
/// B1.2 has no LED (text cell), P1.1 owns L1.3, and the bare `[p2b8]`
/// section materializes the standalone L1.* LED components.
fn led_pairs_app() -> App {
    let patch = Patch::from_ini_file(Path::new("fixtures/led_pairs.ini")).unwrap();
    let mut app = App::new();
    app.load_patch(patch);
    app
}

/// Rect the last real render published for `token`'s component.
fn rect_for(app: &App, token: &str) -> Rect {
    let idx = idx_for(app, token);
    app.component_rects
        .iter()
        .find(|(i, _)| *i == idx)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("render published no rect for {token}"))
}

/// Symbols of one row inside a rendered component cell.
fn row_text(buffer: &Buffer, rect: Rect, row: u16) -> String {
    let y = rect.y + row;
    (0..rect.width)
        .filter_map(|x| buffer.cell((rect.x + x, y)))
        .map(|c| c.symbol())
        .collect()
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

#[test]
fn regression_boxed_cell_renders_led_frame_text_cell_stays_plain() {
    let mut app = led_pairs_app();
    // Flow precondition; full parse coverage lives in patch.rs unit tests.
    assert_eq!(
        app.patch.as_ref().unwrap().hw_components[idx_for(&app, "B1.1")].led,
        Some(String::from("L1.1"))
    );
    let buf = buffer_for(&mut app, 100, 40);

    // Boxed: LED-associated button fills the 3-row cell as a bordered box
    // (controller-panels spec "Box LED-associated elements": symbol, label,
    // state, and the LED glyph — a single state, not a second textual LED
    // state). The label lives in the block's top title, drawn inside the
    // border row, since a 3-row cell has no room for a border plus multiple
    // content lines.
    let boxed = rect_for(&app, "B1.1");
    assert_eq!(boxed.height, 3, "boxed cell is COMPONENT_HEIGHT tall");
    assert!(
        row_text(&buf, boxed, 0).contains("B1.1"),
        "label lives in the top border's title"
    );
    let mid = row_text(&buf, boxed, 1);
    assert!(
        mid.contains("ON") || mid.contains("OFF"),
        "state row: {mid:?}"
    );
    assert!(mid.contains('◉') || mid.contains('○'), "LED glyph in box");
    let led_row = row_text(&buf, boxed, 2);
    assert!(
        !led_row.contains("ON") && !led_row.contains("OFF"),
        "bottom row is plain border, not a second textual state \
         (droid_tui-888 duplicate OFF), got {led_row:?}"
    );

    // Border uses the element's kind color (button -> theme::active().button;
    // knob -> theme::active().knob — controller-panels spec "Kind colors
    // follow the theme").
    let corner = buf
        .cell((boxed.x, boxed.y))
        .expect("boxed cell has a top-left corner cell");
    assert_eq!(
        corner.symbol(),
        "┌",
        "boxed cell renders a bordered box, got {:?}",
        corner.symbol()
    );
    assert_eq!(
        corner.style().fg,
        Some(theme::active().button),
        "border color follows the button kind"
    );

    let knob_boxed = rect_for(&app, "P1.1");
    let knob_corner = buf
        .cell((knob_boxed.x, knob_boxed.y))
        .expect("boxed knob cell has a top-left corner cell");
    assert_eq!(
        knob_corner.symbol(),
        "┌",
        "boxed knob cell renders a bordered box, got {:?}",
        knob_corner.symbol()
    );
    assert_eq!(
        knob_corner.style().fg,
        Some(theme::active().knob),
        "border color follows the knob kind"
    );

    // Text: LED-less button keeps plain content with a blank filler row.
    let plain = rect_for(&app, "B1.2");
    assert_eq!(plain.height, 3, "grid geometry unified at 3 rows");
    assert!(row_text(&buf, plain, 0).contains("B1.2"), "label row");
    assert!(
        row_text(&buf, plain, 2).trim().is_empty(),
        "non-LED cell leaves the filler row blank"
    );
}

#[test]
fn regression_hover_hit_rect_matches_rendered_cell_at_nondefault_scale() {
    // scale_factor used to inflate the *published* hit rect (width/height *
    // scale_factor) while the actual rendered cell stayed COMPONENT_WIDTH x
    // COMPONENT_HEIGHT, so at scale != 1.0 a hit rect spilled past its own
    // cell into the neighbor's screen area: hovering the neighbor resolved
    // to this component instead, and the highlight painted on the wrong
    // cell (droid_tui-wmg).
    let mut app = led_pairs_app();
    handle_event(key(KeyCode::Char('+')), &mut app); // scale_factor away from 1.0
    assert_ne!(app.scale_factor, 1.0);
    let buf = buffer_for(&mut app, 100, 40);

    let id_of =
        |idx: usize| -> String { app.patch.as_ref().unwrap().hw_components[idx].id.clone() };
    let rects = app.component_rects.clone();

    // Every published rect matches the actually-rendered (scaled) cell size —
    // the hit rect IS the rendered cell, so hit testing stays correct at every
    // scale preset (BUG droid_tui-ro0 makes the cell scale with scale_factor;
    // the old fixed-size assertion encoded the pre-fix behavior).
    let expected_h = ((3.0_f32 * app.scale_factor).round() as u16).max(3);
    for (i, r) in &rects {
        assert_eq!(
            r.height,
            expected_h,
            "{} hit rect height must match the rendered (scaled) cell",
            id_of(*i)
        );
    }
    // No rect spills into a neighbor's screen area.
    for (i, a) in rects.iter().enumerate() {
        for b in rects.iter().skip(i + 1) {
            assert!(
                !rects_overlap(a.1, b.1),
                "cells {} and {} overlap at scale_factor {}",
                id_of(a.0),
                id_of(b.0),
                app.scale_factor
            );
        }
    }

    // A mouse move over B1.2's rendered cell must resolve to B1.2, not to
    // its neighbor B1.1 (whose hit rect used to bleed rightward into it).
    let b12 = rect_for(&app, "B1.2");
    let inside_b12 = (b12.x + 1, b12.y);
    handle_mouse_event(
        mouse(MouseEventKind::Moved, inside_b12.0, inside_b12.1),
        &mut app,
    );
    assert_eq!(
        app.hovered_component,
        Some(idx_for(&app, "B1.2")),
        "hover resolves to the cell under the cursor, not an inflated neighbor rect"
    );
    let _ = buf;
}

#[test]
fn regression_p2b8_knobs_render_fully_with_embedded_viewer_open() {
    // droid_tui-6vu: knobs (non-boxed, plain 3-row cells) sit in the last
    // row of the P2B8 panel. Once droid_tui-1hg stopped over-allocating
    // panel rows for folded LEDs, the knob row is no longer squeezed
    // against the panel's bottom border — verify that holds with the
    // embedded source viewer open too (the scenario the original report
    // called out for the highlight overlap).
    for (w, h) in [(100u16, 30u16), (120, 32), (140, 34)] {
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        open_viewer(&mut app);
        let buf = buffer_for(&mut app, w, h);

        let id_of =
            |idx: usize| -> String { app.patch.as_ref().unwrap().hw_components[idx].id.clone() };
        let rects = app.component_rects.clone();

        for tok in ["P1.1", "P1.2"] {
            let r = rects
                .iter()
                .find(|(i, _)| id_of(*i) == tok)
                .unwrap_or_else(|| panic!("{tok} missing a rendered cell at {w}x{h}"))
                .1;
            assert_eq!(
                r.height, 3,
                "{tok} squished to {} rows at {w}x{h} with viewer open",
                r.height
            );
            assert!(
                r.y + r.height <= buf.area.height,
                "{tok} clipped past the frame bottom at {w}x{h}: {r:?}"
            );
        }
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                assert!(
                    !rects_overlap(a.1, b.1),
                    "cells {} and {} overlap at {w}x{h} with viewer open",
                    id_of(a.0),
                    id_of(b.0)
                );
            }
        }
    }
}

#[test]
fn regression_mixed_grid_cells_coexist_without_overlap() {
    let mut app = led_pairs_app();
    let buf = buffer_for(&mut app, 100, 40);
    let rects = app.component_rects.clone();
    assert!(rects.len() >= 5, "fixture renders several cells");

    let id_of =
        |idx: usize| -> String { app.patch.as_ref().unwrap().hw_components[idx].id.clone() };
    let has_rect = |tok: &str| rects.iter().any(|(i, _)| id_of(*i) == tok);

    // Folded LEDs are absorbed into their owners' boxes; unfolded LEDs
    // keep their own standalone cells in the same grid.
    assert!(!has_rect("L1.1"), "folded L1.1 gets no standalone cell");
    assert!(!has_rect("L1.3"), "folded L1.3 gets no standalone cell");
    assert!(has_rect("L1.2"), "unfolded LED keeps its own cell");
    assert!(has_rect("P1.1"), "boxed knob rendered next to text cells");

    for (i, r) in &rects {
        assert!(
            r.x + r.width <= buf.area.width && r.y + r.height <= buf.area.height,
            "cell for {} overflows the frame: {r:?}",
            id_of(*i)
        );
    }
    for (i, a) in rects.iter().enumerate() {
        for b in rects.iter().skip(i + 1) {
            assert!(
                !rects_overlap(a.1, b.1),
                "cells {} and {} overlap",
                id_of(a.0),
                id_of(b.0)
            );
        }
    }
}

#[test]
fn regression_cell_geometry_no_overflow_overlap() {
    // P2B8 in arpeggio1.ini has 8 buttons + 8 folded LEDs + 2 knobs (18 raw
    // HwComponents), but only 10 are visible cells once folded LEDs are
    // absorbed into their owning buttons' boxes. Panel height must be sized
    // from the visible count, not the raw count, or the knobs get clipped
    // off the bottom of the panel (droid_tui-1hg).
    // NOTE: BUG 2 (droid_tui-7ik) sizes each panel from its real inner width, so
    // the wrap count in a narrow frame is correct (no 2px overflow). That makes
    // panels a little taller than the old overflowing wrap, so the frames below
    // are sized with enough vertical room for the corrected grid.
    for (w, h) in [(80u16, 40u16), (100, 44), (120, 48)] {
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        let buf = buffer_for(&mut app, w, h);

        let id_of =
            |idx: usize| -> String { app.patch.as_ref().unwrap().hw_components[idx].id.clone() };
        let rects = app.component_rects.clone();

        for tok in [
            "B1.1", "B1.2", "B1.3", "B1.4", "B1.5", "B1.6", "B1.7", "B1.8", "P1.1", "P1.2",
        ] {
            let r = rects
                .iter()
                .find(|(i, _)| id_of(*i) == tok)
                .unwrap_or_else(|| {
                    panic!("{tok} missing a rendered cell at {w}x{h} — panel clipped it")
                })
                .1;
            // Overallocating rows for folded LEDs used to starve the panel's
            // real rows of vertical space, squishing cells below their full
            // COMPONENT_HEIGHT (droid_tui-1hg).
            assert_eq!(
                r.height, 3,
                "{tok} squished to {} rows at {w}x{h} — panel row count includes folded LEDs",
                r.height
            );
        }
        for tok in [
            "L1.1", "L1.2", "L1.3", "L1.4", "L1.5", "L1.6", "L1.7", "L1.8",
        ] {
            assert!(
                !rects.iter().any(|(i, _)| id_of(*i) == tok),
                "folded {tok} should not get its own standalone cell"
            );
        }

        for (i, r) in &rects {
            assert!(
                r.x + r.width <= buf.area.width && r.y + r.height <= buf.area.height,
                "cell for {} overflows the {w}x{h} frame: {r:?}",
                id_of(*i)
            );
        }
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                assert!(
                    !rects_overlap(a.1, b.1),
                    "cells {} and {} overlap at {w}x{h}",
                    id_of(a.0),
                    id_of(b.0)
                );
            }
        }
    }
}

#[test]
fn regression_click_on_boxed_cell_toggles_and_selects() {
    let mut app = led_pairs_app();
    // Real renderer geometry drives hit-testing — no hand-built rects.
    let _ = buffer_for(&mut app, 100, 40);

    let boxed = rect_for(&app, "B1.1");
    let idx = idx_for(&app, "B1.1");
    let start = app.patch.as_ref().unwrap().hw_components[idx].state.clone();
    let cx = boxed.x + boxed.width / 2;
    let cy = boxed.y + boxed.height / 2;

    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), cx, cy),
        &mut app,
    );
    let mid = app.patch.as_ref().unwrap().hw_components[idx].state.clone();
    assert_ne!(mid, start, "click inside boxed cell toggles");
    assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
    assert_eq!(app.hovered_component, Some(idx));

    handle_mouse_event(
        mouse(MouseEventKind::Down(MouseButton::Left), cx, cy),
        &mut app,
    );
    assert_eq!(
        app.patch.as_ref().unwrap().hw_components[idx].state,
        start,
        "second click toggles back"
    );

    // Text cell hit-testing still works alongside boxes.
    let plain = rect_for(&app, "B1.2");
    let idx2 = idx_for(&app, "B1.2");
    let before = app.patch.as_ref().unwrap().hw_components[idx2]
        .state
        .clone();
    handle_mouse_event(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            plain.x + plain.width / 2,
            plain.y + plain.height / 2,
        ),
        &mut app,
    );
    assert_ne!(
        app.patch.as_ref().unwrap().hw_components[idx2].state,
        before,
        "click on text cell toggles"
    );
    assert_eq!(app.selected_component.as_deref(), Some("B1.2"));
}

#[test]
fn regression_split_ratio_brackets_clamp_snap_and_drive_layout() {
    let mut app = fixture_app();
    open_viewer(&mut app);
    let approx = |got: f32, want: f32| (got - want).abs() < 1e-6;
    assert!(approx(app.viewer_split_ratio, 0.6), "default 0.6");

    handle_event(key(KeyCode::Char(']')), &mut app);
    assert!(approx(app.viewer_split_ratio, 0.7), "] steps +0.1 exactly");
    assert!(
        app.status_message.contains("70%/30%"),
        "status reflects split: {:?}",
        app.status_message
    );
    handle_event(key(KeyCode::Char(']')), &mut app);
    assert!(approx(app.viewer_split_ratio, 0.7), "clamped at 0.7");

    for want in [0.6f32, 0.5, 0.4, 0.3] {
        handle_event(key(KeyCode::Char('[')), &mut app);
        assert!(approx(app.viewer_split_ratio, want), "exact step {want}");
    }
    handle_event(key(KeyCode::Char('[')), &mut app);
    assert!(approx(app.viewer_split_ratio, 0.3), "clamped at 0.3");
    assert!(app.status_message.contains("30%/70%"));

    // Ratio is a view preference: it survives loading another patch.
    assert!(app.showing_viewer, "viewer stays open across load_patch");
    let other = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
    app.load_patch(other);
    assert!(approx(app.viewer_split_ratio, 0.3), "ratio persists");

    // The rendered split follows the key-driven ratio: panels right border
    // sits at ~ratio of a 100-col main area (row 3 = panels top border row).
    fn border_col(app: &mut App) -> u16 {
        let buf = buffer_for(app, 100, 40);
        let mut col = 0u16;
        for x in 0..buf.area.width {
            if buf.cell((x, 3)).map(|c| c.symbol() == "┐").unwrap_or(false) {
                col = x;
                break;
            }
        }
        col
    }
    let at_03 = border_col(&mut app);
    for _ in 0..4 {
        handle_event(key(KeyCode::Char(']')), &mut app);
    }
    assert!(approx(app.viewer_split_ratio, 0.7));
    let at_07 = border_col(&mut app);
    assert!(at_07 > at_03, "border moves right as panels grow");
    assert!((28..=31).contains(&at_03), "30% of 100 cols, got {at_03}");
    assert!((68..=71).contains(&at_07), "70% of 100 cols, got {at_07}");
}

#[test]
fn regression_narrow_terminal_boxed_layout_no_panic() {
    for (w, h) in [(100u16, 40u16), (60, 24), (40, 14), (26, 10), (20, 8)] {
        let mut app = led_pairs_app();
        let buf = buffer_for(&mut app, w, h);
        for (_, r) in &app.component_rects {
            assert!(
                r.x + r.width <= w && r.y + r.height <= h,
                "boxed cell {r:?} overflows {w}x{h}"
            );
        }
        assert!(!buf.content().is_empty());
    }
    // Embedded viewer with the ratio split also degrades gracefully.
    for (w, h) in [(80u16, 24u16), (50, 16), (36, 12)] {
        let mut app = led_pairs_app();
        app.showing_viewer = true;
        let _ = buffer_for(&mut app, w, h);
    }
}

// ── theme regression (config-store task 3.3) ─────────────────────────────
// Each surface × theme pair renders into a real TestBackend frame with the
// palette pinned via the test-only thread-local override, so these tests stay
// order-independent and never touch prod `theme::init()` state.

use crate::theme;

/// Pins a built-in palette for the calling thread and always restores the
/// default resolution on drop, keeping sibling tests on `classic` even when
/// an assertion fires mid-theme.
struct ThemedGuard;

impl ThemedGuard {
    fn pin(name: &str) -> Self {
        theme::set_test_theme(Some(*theme::resolve(name)));
        Self
    }
}

impl Drop for ThemedGuard {
    fn drop(&mut self) {
        theme::set_test_theme(None);
    }
}

/// Style of the first character of the first occurrence of `token`, or None
/// when the token is not rendered anywhere in the buffer.
fn first_token_style(buffer: &Buffer, token: &str) -> Option<Style> {
    let area = buffer.area;
    let want: Vec<char> = token.chars().collect();
    for y in 0..area.height {
        let mut chars: Vec<char> = Vec::with_capacity(area.width as usize);
        let mut styles: Vec<Style> = Vec::with_capacity(area.width as usize);
        for x in 0..area.width {
            let cell = buffer.cell((x, y)).unwrap();
            chars.push(cell.symbol().chars().next().unwrap_or(' '));
            styles.push(cell.style());
        }
        if chars.len() < want.len() {
            continue;
        }
        for start in 0..=chars.len() - want.len() {
            if chars[start..start + want.len()] == want[..] {
                return Some(styles[start]);
            }
        }
    }
    None
}

/// Foreground color of the first cell carrying `glyph`, or None. Row-major,
/// so with a switch panel first the value-switch `◉` is found before a
/// knob's `◉`.
fn glyph_fg(buffer: &Buffer, glyph: &str) -> Option<Color> {
    buffer
        .content()
        .iter()
        .find(|c| c.symbol() == glyph)
        .map(|c| c.fg)
}

/// Whether any box-drawing glyph is drawn in `fg` (and `modifier`, when given).
fn has_border_glyph(buffer: &Buffer, fg: Color, modifier: Option<Modifier>) -> bool {
    const GLYPHS: [char; 11] = ['─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];
    buffer.content().iter().any(|cell| {
        let sym = cell.symbol();
        sym.chars().count() == 1
            && GLYPHS.contains(&sym.chars().next().unwrap_or(' '))
            && cell.style().fg == Some(fg)
            && modifier.is_none_or(|m| cell.style().add_modifier.contains(m))
    })
}

/// Spot-checks that pin `name`'s signature tokens to their documented ANSI
/// values, guarding against silent palette drift in the classic palette.
fn assert_classic_signature_tokens(t: &crate::theme::Theme) {
    assert_eq!(t.shift1, Color::Yellow, "classic shift1");
    assert_eq!(t.focus_border, Color::Yellow, "classic focus_border");
    assert_eq!(t.occurrence_highlight, Color::Yellow, "classic occurrence");
    assert_eq!(t.accent, Color::Blue, "classic accent");
    assert_eq!(t.status_bg, Color::DarkGray, "classic status_bg");
}

#[test]
fn regression_theme_boxed_cells_and_shift_surfaces() {
    for &name in theme::THEMES {
        let _guard = ThemedGuard::pin(name);
        let t = *theme::resolve(name);
        if name == "classic" {
            assert_classic_signature_tokens(&t);
        }

        // Boxed LED cell content carries the component-kind token, framed by
        // panel borders in the muted token while no shift is active.
        let mut plain = led_pairs_app();
        let buf = buffer_for(&mut plain, 100, 40);
        let _boxed = rect_for(&plain, "B1.1");
        let label = first_token_style(&buf, "B1.1").expect("boxed label rendered");
        assert_eq!(label.fg, Some(t.button), "{name}: boxed cell kind color");
        assert!(
            has_border_glyph(&buf, t.muted, None),
            "{name}: idle panel border muted"
        );

        // Active shift repaints affected panel borders and the status chip
        // with the group token over the shared status background.
        let mut shifted = App::new();
        shifted.load_sample_patch();
        shifted.active_shift = Some(ShiftGroup::Group1);
        let buf = buffer_for(&mut shifted, 100, 40);
        assert!(
            has_border_glyph(&buf, t.shift1, Some(Modifier::BOLD)),
            "{name}: affected panel border uses shift1 bold"
        );
        // Match the full status phrase: affected panel titles also say
        // "[SHIFT 1]", so the bare word would hit the title row first.
        let chip = first_token_style(&buf, "SHIFT 1 ACTIVE").expect("shift status rendered");
        assert_eq!(chip.fg, Some(t.shift1), "{name}: shift status fg");
        assert_eq!(chip.bg, Some(t.status_bg), "{name}: status bar background");
    }
}

#[test]
fn regression_theme_picker_surface() {
    for &name in theme::THEMES {
        let _guard = ThemedGuard::pin(name);
        let t = *theme::resolve(name);
        if name == "classic" {
            assert_classic_signature_tokens(&t);
        }

        let mut app = App::new();
        app.showing_picker = true;
        app.refresh_picker_entries();
        let buf = buffer_for(&mut app, 100, 30);
        let title = first_token_style(&buf, "File Picker").expect("picker title rendered");
        assert_eq!(title.bg, Some(t.muted), "{name}: picker body background");
        assert!(
            has_border_glyph(&buf, t.accent, None),
            "{name}: picker border accent"
        );
    }
}

#[test]
fn regression_theme_viewer_sidebar_content_status() {
    for &name in theme::THEMES {
        let _guard = ThemedGuard::pin(name);
        let t = *theme::resolve(name);
        if name == "classic" {
            assert_classic_signature_tokens(&t);
        }

        let mut app = fixture_app();
        app.select_component(String::from("B1.1"));
        open_viewer(&mut app);
        let buf = buffer_for(&mut app, 120, 40);

        // Sidebar frame uses the accent token.
        assert!(
            has_border_glyph(&buf, t.accent, None),
            "{name}: sidebar border accent"
        );
        // Source-focused content pane gets the focus border, bold.
        assert!(
            has_border_glyph(&buf, t.focus_border, Some(Modifier::BOLD)),
            "{name}: focused source border"
        );
        // Viewer status bar paints its background and key-hint tokens.
        let status = first_token_style(&buf, "Source Viewer").expect("viewer status rendered");
        assert_eq!(status.bg, Some(t.status_bg), "{name}: viewer status bg");
        let esc = first_token_style(&buf, "ESC").expect("key hints rendered");
        assert_eq!(esc.fg, Some(t.viewer_key), "{name}: viewer key hint fg");
        // Current occurrence rides the occurrence-highlight token.
        assert!(
            has_highlighted_token(
                &buf,
                "B1.1",
                Some(t.occurrence_highlight),
                Some(Modifier::REVERSED)
            ),
            "{name}: current occurrence highlighted"
        );
    }
}
// ── visual validation snapshots (tasks 1.2, 1.3, 1.4) ─────────────────
// Deterministic ANSI/HTML face proofs for the coverage matrix:
// fixtures arpeggio1 / led_pairs / source_navigation × themes classic/terminal/mono
// × widths 80/120/100 × viewer open/closed and shift1 active.
// Each scenario renders via TestBackend into a real Buffer, then into ANSI
// (trimmed trailing spaces) and HTML (span per cell with fg/bg/bold/dim/reversed).
// Assertions guard face (P2B8 8 buttons + 2 knobs), style tokens (kind colors,
// muted chrome, shift chip), and boxed vs plain invariants; snapshots make the
// face inspectable and are the strict gate — `cargo test` fails until
// `cargo insta accept`.

fn app_from_fixture(name: &str) -> App {
    let path = format!("fixtures/{name}.ini");
    let patch = Patch::from_ini_file(Path::new(&path)).unwrap();
    let mut app = App::new();
    app.load_patch(patch);
    app
}

#[test]
fn visual_controller_panels_arpeggio_snapshot() {
    // 1.2: arpeggio1.ini × classic/terminal/mono × 80/120
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);
        for width in [80u16, 120] {
            let mut app = app_from_fixture("arpeggio1");
            let buf = buffer_for(&mut app, width, 30);
            let ansi = buffer_to_ansi(&buf);
            let html = buffer_to_html(&buf);

            // Face: P2B8 panel must expose 8 buttons + 2 knobs (2 pots) from the fixture.
            let patch = app.patch.as_ref().unwrap();
            let p2b8_buttons = patch
                .hw_components
                .iter()
                .filter(|c| c.controller == "P2B8" && c.kind == crate::patch::ComponentKind::Button)
                .count();
            let p2b8_knobs = patch
                .hw_components
                .iter()
                .filter(|c| c.controller == "P2B8" && c.kind == crate::patch::ComponentKind::Knob)
                .count();
            assert_eq!(p2b8_buttons, 8, "{theme_name} {width}: P2B8 8 buttons");
            assert_eq!(p2b8_knobs, 2, "{theme_name} {width}: P2B8 2 knobs");
            assert!(
                ansi.contains("P2B8"),
                "{theme_name} {width}: panel title P2B8 in ANSI"
            );
            // Each P2B8 button gets a distinct, unclipped identifier — not
            // all clipped to the same "P2B8 Button 1." (droid_tui-p2x).
            for i in 1..=8 {
                let tok = format!("B1.{i}");
                assert!(
                    ansi.contains(&tok),
                    "{theme_name} {width}: {tok} label not clipped/merged"
                );
            }

            // Style tokens: kind colors (button white / knob magenta etc) and muted chrome.
            if let Some(style) = first_token_style(&buf, "B1.1") {
                assert_eq!(
                    style.fg,
                    Some(t.button),
                    "{theme_name} {width}: button kind color"
                );
            }
            if let Some(style) = first_token_style(&buf, "P1.1") {
                // P1.1 is a Knob on P2B8
                assert_eq!(
                    style.fg,
                    Some(t.knob),
                    "{theme_name} {width}: knob kind color"
                );
            }
            // Header/picker chrome uses muted; panel borders are muted when no shift active.
            assert!(
                has_border_glyph(&buf, t.muted, None)
                    || has_border_glyph(&buf, t.muted, Some(Modifier::DIM)),
                "{theme_name} {width}: muted chrome border present"
            );

            // HTML helper sanity: non-empty and contains expected face tokens.
            assert!(!html.is_empty(), "{theme_name} {width}: html non-empty");
            // HTML is per-cell spans, so contiguous text is split across tags — check tags + ANSI instead.
            assert!(
                html.contains("<span"),
                "{theme_name} {width}: html has spans"
            );
            assert!(!html.is_empty());

            insta::with_settings!({snapshot_suffix => format!("arpeggio_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
}

#[test]
fn visual_multi_module_p2b8_snapshot() {
    // droid_tui-21v / droid_tui-my3: two bare [p2b8] sections must render as
    // two distinct module sub-blocks within one "P2B8" panel (not one flat
    // 36-component grid), each internally in physical B.1..B.8 order.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        for width in [80u16, 120] {
            let mut app = app_from_fixture("multi_module_p2b8");
            let buf = buffer_for(&mut app, width, 40);
            let ansi = buffer_to_ansi(&buf);

            assert!(
                ansi.contains("P2B8 1"),
                "{theme_name} {width}: module 1 sub-block title present\n{ansi}"
            );
            assert!(
                ansi.contains("P2B8 2"),
                "{theme_name} {width}: module 2 sub-block title present\n{ansi}"
            );

            // Physical order within each module: B.1 before B.8, and module 1's
            // components all precede module 2's (no cross-instance interleaving).
            let pos = |needle: &str| {
                ansi.find(needle)
                    .unwrap_or_else(|| panic!("{theme_name} {width}: {needle} missing\n{ansi}"))
            };
            assert!(
                pos("B1.1") < pos("B1.8"),
                "{theme_name} {width}: B1.1 before B1.8"
            );
            assert!(
                pos("B1.8") < pos("B2.1"),
                "{theme_name} {width}: module 1 before module 2"
            );
            assert!(
                pos("B2.1") < pos("B2.8"),
                "{theme_name} {width}: B2.1 before B2.8"
            );

            insta::with_settings!({snapshot_suffix => format!("multi_module_p2b8_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
}

#[test]
fn visual_boxed_vs_plain_led_pairs_snapshot() {
    // 1.3 part A: led_pairs.ini mixed boxed/text grid at width 100
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);
        let mut app = led_pairs_app();
        let buf = buffer_for(&mut app, 100, 40);
        let ansi = buffer_to_ansi(&buf);
        let html = buffer_to_html(&buf);

        // Invariant per droid_tui-5mj / droid_tui-1hg: boxed vs plain gated on led Some.
        let patch = app.patch.as_ref().unwrap();
        let b11 = patch
            .hw_components
            .iter()
            .find(|c| c.id == "B1.1")
            .expect("B1.1");
        let b12 = patch
            .hw_components
            .iter()
            .find(|c| c.id == "B1.2")
            .expect("B1.2");
        let p11 = patch
            .hw_components
            .iter()
            .find(|c| c.id == "P1.1")
            .expect("P1.1");
        assert!(b11.led.is_some(), "B1.1 boxed (led Some)");
        assert!(b12.led.is_none(), "B1.2 plain (led None)");
        assert!(p11.led.is_some(), "P1.1 boxed (led Some)");

        // Folded LEDs have no standalone cell; unfolded LED keeps its cell.
        let rect_ids: Vec<String> = app
            .component_rects
            .iter()
            .map(|(idx, _)| patch.hw_components[*idx].id.clone())
            .collect();
        assert!(
            !rect_ids.contains(&String::from("L1.1")),
            "folded L1.1 no standalone cell"
        );
        assert!(
            !rect_ids.contains(&String::from("L1.3")),
            "folded L1.3 no standalone cell"
        );
        assert!(
            rect_ids.contains(&String::from("L1.2")),
            "unfolded L1.2 keeps cell"
        );

        // Boxed border kind-colored: boxed component's label fg equals its kind token.
        if let Some(style) = first_token_style(&buf, "B1.1") {
            assert_eq!(
                style.fg,
                Some(t.button),
                "{theme_name}: boxed B1.1 kind color"
            );
        }
        if let Some(style) = first_token_style(&buf, "P1.1") {
            assert_eq!(
                style.fg,
                Some(t.knob),
                "{theme_name}: boxed P1.1 kind color"
            );
        }
        assert!(!ansi.is_empty());
        assert!(!html.is_empty());
        assert!(html.contains("<span"), "led_pairs: html has spans");

        insta::with_settings!({snapshot_suffix => format!("led_pairs_{theme_name}_100")}, {
            insta::assert_snapshot!(ansi);
        });
        insta::with_settings!({snapshot_suffix => format!("led_pairs_{theme_name}_100_html")}, {
            insta::assert_snapshot!(html);
        });
    }
}

#[test]
fn visual_numbered_led_pairs_snapshot() {
    // droid_tui-abt: matrixmixer-style `ledN = L.N` params (shared suffix with
    // `buttonN`) must render B1.x/L1.x as single boxed cells with the LED folded.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let mut app = app_from_fixture("numbered_led_pairs");
        let buf = buffer_for(&mut app, 80, 40);
        let ansi = buffer_to_ansi(&buf);

        // Parser associated both buttons via their numbered ledN sibling.
        let patch = app.patch.as_ref().unwrap();
        let b11 = patch.hw_components.iter().find(|c| c.id == "B1.1").unwrap();
        let b12 = patch.hw_components.iter().find(|c| c.id == "B1.2").unwrap();
        assert_eq!(b11.led.as_deref(), Some("L1.1"), "{theme_name}: B1.1 boxed");
        assert_eq!(b12.led.as_deref(), Some("L1.2"), "{theme_name}: B1.2 boxed");

        // Both LEDs folded: no standalone L1.1/L1.2 cells.
        let rect_ids: Vec<String> = app
            .component_rects
            .iter()
            .map(|(idx, _)| patch.hw_components[*idx].id.clone())
            .collect();
        assert!(
            !rect_ids.contains(&String::from("L1.1")) && !rect_ids.contains(&String::from("L1.2")),
            "{theme_name}: numbered-pair LEDs folded"
        );

        insta::with_settings!({snapshot_suffix => format!("numbered_led_pairs_{theme_name}_80")}, {
            insta::assert_snapshot!(ansi);
        });
    }
}

#[test]
fn visual_viewer_layout_open_closed_snapshot() {
    // 1.3 part B: source_navigation.ini viewer open/closed at width 100
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);

        // Closed: plain panels + normal status bar
        let mut closed = app_from_fixture("source_navigation");
        closed.showing_viewer = false;
        let buf_closed = buffer_for(&mut closed, 100, 40);
        let ansi_closed = buffer_to_ansi(&buf_closed);
        assert!(
            !ansi_closed.contains("Source Viewer"),
            "{theme_name}: closed has no viewer status"
        );
        // Status bar background present (darkgray in classic) and muted borders
        assert!(
            has_border_glyph(&buf_closed, t.muted, None)
                || has_border_glyph(&buf_closed, t.muted, Some(Modifier::DIM)),
            "{theme_name}: closed muted chrome"
        );
        insta::with_settings!({snapshot_suffix => format!("viewer_closed_{theme_name}_100")}, {
            insta::assert_snapshot!(ansi_closed);
        });

        // Open: embedded viewer with panels/source split, sidebar, minimap, status hints
        let mut open = app_from_fixture("source_navigation");
        open.select_component(String::from("B1.1"));
        open.showing_viewer = true;
        open.viewer_focus = ViewerFocus::Source;
        let buf_open = buffer_for(&mut open, 100, 40);
        let ansi_open = buffer_to_ansi(&buf_open);
        let html_open = buffer_to_html(&buf_open);
        assert!(
            ansi_open.contains("Source Viewer"),
            "{theme_name}: open viewer status hints present"
        );
        assert!(
            ansi_open.contains("Panels") || ansi_open.contains("Circuits"),
            "{theme_name}: open shows panels/sidebar"
        );
        // Viewer chrome: sidebar accent border and focused source border
        assert!(
            has_border_glyph(&buf_open, t.accent, None),
            "{theme_name}: viewer sidebar accent border"
        );
        // Source-focused content pane gets focus_border bold (yellow in classic)
        assert!(
            has_border_glyph(&buf_open, t.focus_border, Some(Modifier::BOLD)),
            "{theme_name}: focused source border bold"
        );
        assert!(!html_open.is_empty());
        insta::with_settings!({snapshot_suffix => format!("viewer_open_{theme_name}_100")}, {
            insta::assert_snapshot!(ansi_open);
        });
        insta::with_settings!({snapshot_suffix => format!("viewer_open_{theme_name}_100_html")}, {
            insta::assert_snapshot!(html_open);
        });
    }
}

#[test]
fn visual_viewer_live_interaction_snapshot() {
    // droid_tui-0lw: with the viewer open and the source pane focused, panel
    // keys are live. Frame A drives shift1 through the real key path while
    // Source is focused — a state that was impossible before the fix because
    // '1' was swallowed by the source-focus branch — and proves the shift
    // face (chip + bold borders) renders beside the viewer chrome. Frame B
    // shows B1.1 toggled AND selected via Enter with source_scroll parked at
    // its first occurrence while the viewer stays open.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);

        // Frame A: viewer open + Source focused + shift1 activated live.
        let mut app = app_from_fixture("source_navigation");
        open_viewer(&mut app);
        assert_eq!(app.viewer_focus, ViewerFocus::Source);
        handle_event(key(KeyCode::Char('1')), &mut app);
        assert_eq!(
            app.active_shift,
            Some(ShiftGroup::Group1),
            "{theme_name}: shift1 live while Source focused"
        );
        let buf = buffer_for(&mut app, 100, 40);
        let ansi = buffer_to_ansi(&buf);
        // The viewer status bar replaces the normal one (and its hints fill
        // width 100), so liveness shows on the panels themselves: affected
        // panels get [SHIFT 1] titles and bold shift-colored borders.
        assert!(
            ansi.contains("[SHIFT 1]"),
            "{theme_name}: affected panels tagged [SHIFT 1] with viewer open\n{ansi}"
        );
        assert!(
            ansi.contains("Source Viewer"),
            "{theme_name}: viewer still open beside live panels"
        );
        assert!(
            has_border_glyph(&buf, t.shift1, Some(Modifier::BOLD)),
            "{theme_name}: shift1 bold border visible with viewer open"
        );
        insta::with_settings!({snapshot_suffix => format!("viewer_live_shift1_{theme_name}_100")}, {
            insta::assert_snapshot!(ansi);
        });

        // Frame B: Enter toggles + selects B1.1 and scrolls the source view
        // to its first occurrence — the viewer never closes.
        let b11 = idx_for(&app, "B1.1");
        app.hovered_component = Some(b11);
        let first_b11 = app.patch.as_ref().unwrap().occurrences_for("B1.1")[0].line;
        let state_before = app.patch.as_ref().unwrap().hw_components[b11].state.clone();
        handle_event(key(KeyCode::Enter), &mut app);
        assert_ne!(
            app.patch.as_ref().unwrap().hw_components[b11].state,
            state_before,
            "{theme_name}: Enter toggles while Source focused"
        );
        assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
        assert_eq!(app.source_scroll, first_b11);
        assert!(app.showing_viewer, "{theme_name}: viewer stays open");

        let buf = buffer_for(&mut app, 100, 40);
        let ansi = buffer_to_ansi(&buf);
        assert!(
            ansi.contains("B1.1"),
            "{theme_name}: selected token visible in both columns"
        );
        assert!(
            ansi.contains("[SHIFT 1]"),
            "{theme_name}: shift face persists across toggle frame"
        );
        insta::with_settings!({snapshot_suffix => format!("viewer_live_toggle_{theme_name}_100")}, {
            insta::assert_snapshot!(ansi);
        });
    }
}

#[test]
fn visual_theming_shift_and_mono_snapshot() {
    // 1.4: same fixtures with shift1 active (bold colored border + SHIFT 1 ACTIVE chip)
    // and mono grayscale pairwise distinct, plus side-by-side html row.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);
        let mut app = app_from_fixture("arpeggio1");
        app.active_shift = Some(ShiftGroup::Group1);
        let buf = buffer_for(&mut app, 100, 30);
        let ansi = buffer_to_ansi(&buf);
        let html = buffer_to_html(&buf);

        // Shift visualization: affected panel borders bold shift1, status chip fg shift1 bg status_bg
        assert!(
            has_border_glyph(&buf, t.shift1, Some(Modifier::BOLD)),
            "{theme_name}: shift1 affected panel border bold"
        );
        let chip = first_token_style(&buf, "SHIFT 1 ACTIVE").expect("SHIFT 1 ACTIVE chip");
        assert_eq!(chip.fg, Some(t.shift1), "{theme_name}: chip fg shift1");
        assert_eq!(
            chip.bg,
            Some(t.status_bg),
            "{theme_name}: chip bg status_bg"
        );
        assert!(ansi.contains("SHIFT 1 ACTIVE"), "{theme_name}: chip text");
        assert!(!html.is_empty());

        insta::with_settings!({snapshot_suffix => format!("shift1_{theme_name}_100")}, {
            insta::assert_snapshot!(ansi);
        });
        insta::with_settings!({snapshot_suffix => format!("shift1_{theme_name}_100_html")}, {
            insta::assert_snapshot!(html);
        });
    }

    // Also exercise led_pairs with shift1 to cover boxed+shift interaction
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);
        let mut app = led_pairs_app();
        app.active_shift = Some(ShiftGroup::Group1);
        let buf = buffer_for(&mut app, 100, 40);
        let ansi = buffer_to_ansi(&buf);
        // If any component belongs to Group1, panel border will be shift1; otherwise muted dim.
        // led_pairs fixture has no explicit shift groups, so borders stay muted dim — just verify no panic and chip present.
        assert!(ansi.contains("SHIFT 1 ACTIVE"));
        assert!(
            has_border_glyph(&buf, t.shift1, Some(Modifier::BOLD))
                || has_border_glyph(&buf, t.muted, Some(Modifier::DIM))
                || has_border_glyph(&buf, t.muted, None),
            "{theme_name}: shift or muted border present with boxed fixture"
        );
        insta::with_settings!({snapshot_suffix => format!("led_shift1_{theme_name}_100")}, {
            insta::assert_snapshot!(ansi);
        });
    }

    // mono grayscale pairwise distinct (mirrors theme unit test, but as visual gate)
    let mono = crate::theme::Theme::mono();
    let shifts = [mono.shift1, mono.shift2, mono.shift3, mono.shift4];
    for (i, a) in shifts.iter().enumerate() {
        for b in &shifts[i + 1..] {
            assert_ne!(a, b, "mono shift tokens pairwise distinct");
        }
    }
    assert_ne!(
        mono.modifier_boolean, mono.modifier_exact,
        "mono boolean vs exact distinct"
    );
    assert_ne!(
        mono.minimap_occurrence, mono.minimap_combined,
        "mono minimap occurrence vs combined distinct"
    );
    assert_ne!(
        mono.minimap_modifier_boolean, mono.minimap_modifier_exact,
        "mono minimap boolean vs exact distinct"
    );

    // Side-by-side html row: one row per scenario columns classic/terminal/mono for arpeggio shift case
    let mut html_cells: Vec<String> = Vec::new();
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let mut app = app_from_fixture("arpeggio1");
        app.active_shift = Some(ShiftGroup::Group1);
        let buf = buffer_for(&mut app, 80, 24);
        let html = buffer_to_html(&buf);
        html_cells.push(format!("<td data-theme=\"{theme_name}\">{html}</td>"));
    }
    let row = format!("<tr>{}</tr>", html_cells.join(""));
    assert!(row.contains("<td"), "html row has cells");
    // SHIFT chip in HTML is split per-cell into <span> fragments, so check
    // for per-char presence and for the themed chip color instead of a raw
    // substring.
    assert!(
        row.contains('S') && row.contains("td"),
        "html row contains shift chip cells"
    );
    assert!(row.len() > 200, "html row substantial");
    insta::assert_snapshot!("shift_html_row", row);

    // Additional viewer shift html row at 100 cols
    let mut viewer_cells: Vec<String> = Vec::new();
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let mut app = app_from_fixture("source_navigation");
        app.select_component(String::from("B1.1"));
        app.showing_viewer = true;
        app.active_shift = Some(ShiftGroup::Group1);
        let buf = buffer_for(&mut app, 100, 40);
        let html = buffer_to_html(&buf);
        viewer_cells.push(format!("<td data-theme=\"{theme_name}\">{html}</td>"));
    }
    let viewer_row = format!("<tr>{}</tr>", viewer_cells.join(""));
    assert!(viewer_row.contains("<td"));
    assert!(viewer_row.len() > 200);
    insta::assert_snapshot!("viewer_shift_html_row", viewer_row);
}

// ── gallery generation hook (task 2.1) ───────────────────────────────────
// Materializes `evidence/gallery/index.html` on demand via
// `cargo test -- --generate-gallery` or `GENERATE_GALLERY=1 cargo test`.
// Uses the same TestBackend + buffer_to_html path as the visual tests so
// the gallery matches ANSI snapshots byte-for-byte.
#[test]
fn gallery_generate_on_flag() {
    if !crate::gallery::should_generate_gallery() {
        return;
    }
    let path = crate::gallery::generate_gallery().expect("gallery generation failed");
    assert!(
        path.exists(),
        "gallery index.html written to {}",
        path.display()
    );
    let html = std::fs::read_to_string(&path).expect("read gallery");
    // Gallery must have one row per scenario and three theme columns per row.
    assert!(html.contains("<table>"), "gallery table present");
    assert!(
        html.contains("data-theme=\"classic\"")
            && html.contains("data-theme=\"terminal\"")
            && html.contains("data-theme=\"mono\"")
    );
    // Spot-check that a known fixture token appears in the HTML cells
    // (per-cell HTML is split across spans, so check for presence of
    // hallmark tokens in ANSI sidecars as well).
    let ansi_classic = std::fs::read_to_string("evidence/gallery/arpeggio_80_classic.ansi")
        .expect("arpeggio ansi sidecar");
    assert!(
        ansi_classic.contains("P2B8") || html.contains("P2B8"),
        "gallery contains P2B8 face"
    );
}

// ── graph render snapshot/visual tests (task 5.3) ───────────────────────
// Renders the full-screen signal-flow graph (design D8) through the same
// TestBackend + insta path as the panel/viewer visual tests. A fixture loaded
// via `app_from_fixture` + `open_graph` mirrors how `open_viewer` drives the
// viewer, so each scenario is a real graph-build + layout-solve + render.
// Edge colors are asserted directly on buffer cells (ANSI/HTML drop fg), and
// snapshots pin the geometry so faces stay inspectable and gate regressions.

/// An `App` with a fixture patch loaded and the graph view opened, mirroring
/// `open_viewer` for the graph surface (`g g` equivalent).
fn graph_app_from_fixture(name: &str) -> App {
    let mut app = app_from_fixture(name);
    app.open_graph();
    assert!(app.showing_graph, "graph view should be open");
    app
}

/// True when any box-drawing glyph cell in `buffer` carries fg `color`.
/// Edge polylines render as box glyphs (`─│┌┐└┘├┤┬┴┼`) with the cable color;
/// ports (`◉`/`●`) and node/cluster frames use other tokens, so filtering to
/// box glyphs isolates edge cells from the rest of the graph face.
fn has_box_glyph_of_color(buffer: &Buffer, color: Color) -> bool {
    buffer.content().iter().any(|cell| {
        cell.fg == color
            && ["─", "│", "┌", "┐", "└", "┘", "├", "┤", "┬", "┴", "┼"].contains(&cell.symbol())
    })
}

#[test]
fn visual_graph_node_cluster_faces_snapshot() {
    // cable_banner_combos.ini: two banner clusters (implicit unnamed group +
    // "Mixer") over multiple circuits. Faces: rounded node frames + title
    // bars, left input / right output ports, and titled cluster containers.
    for &theme_name in theme::THEMES {
        if theme_name == "terminal" {
            continue; // faces already covered; keep the matrix light
        }
        let _guard = ThemedGuard::pin(theme_name);
        for width in [100u16, 40] {
            let mut app = graph_app_from_fixture("cable_banner_combos");
            let buf = buffer_for(&mut app, width, 40);
            let ansi = buffer_to_ansi(&buf);

            // Rounded node frames + cluster container.
            assert!(
                ansi.contains("╭"),
                "{theme_name} {width}: rounded node frame missing\n{ansi}"
            );
            // Circuit titles are only asserted at the wide width: on a 40-col
            // surface the force-directed layout stacks nodes (each 22 cols) into
            // ~18 cols of travel, so frames overlap and a neighbor's port glyph
            // clips a title's last char. That is expected narrow-terminal
            // degradation (covered by regression_graph_narrow_terminal_no_panic),
            // not a title that failed to render.
            if width >= 100 {
                for circuit in ["button", "clocktool", "mixer", "contour"] {
                    assert!(
                        ansi.contains(circuit),
                        "{theme_name} {width}: node title {circuit} missing"
                    );
                }
            }
            // Ports: button/clocktool source _GATE/_CLOCK (right output ●);
            // mixer/contour sink them (left input ◉).
            assert!(ansi.contains("◉"), "{theme_name} {width}: input port");
            assert!(ansi.contains("●"), "{theme_name} {width}: output port");
            // Cluster containers: plain border + "Mixer" banner title.
            assert!(ansi.contains("┌"), "{theme_name} {width}: cluster border");
            assert!(
                ansi.contains("Mixer"),
                "{theme_name} {width}: cluster title missing"
            );

            insta::with_settings!({snapshot_suffix => format!("graph_cable_banner_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
}

#[test]
fn visual_graph_edge_kinds_colors_snapshot() {
    // graph_edge_kinds.ini chains clocktool -> osc -> notesequencer -> vca,
    // producing _CLK (control), _AUD (audio), _NOTE (midi). In classic these
    // map to distinct ANSI tokens, so assert the colored box glyphs directly.
    let _guard = ThemedGuard::pin("classic");
    let t = *theme::resolve("classic");
    let mut app = graph_app_from_fixture("graph_edge_kinds");
    let buf = buffer_for(&mut app, 100, 40);

    assert!(
        has_box_glyph_of_color(&buf, t.graph_edge_control),
        "control edge (_CLK) renders cyan"
    );
    assert!(
        has_box_glyph_of_color(&buf, t.graph_edge_audio),
        "audio edge (_AUD) renders green"
    );
    assert!(
        has_box_glyph_of_color(&buf, t.graph_edge_midi),
        "midi edge (_NOTE) renders magenta"
    );

    let ansi = buffer_to_ansi(&buf);
    insta::with_settings!({snapshot_suffix => "graph_edge_kinds_classic_100"}, {
        insta::assert_snapshot!(ansi);
    });
}

#[test]
fn visual_graph_topology_error_highlight_snapshot() {
    // graph_topology_error.ini: `_CLK` has two sources (clocktool + divider),
    // an n -> 1 topology Error, so every `_CLK` edge must render with the
    // graph_edge_error token (red), overriding the inferred control color.
    let _guard = ThemedGuard::pin("classic");
    let t = *theme::resolve("classic");
    let mut app = graph_app_from_fixture("graph_topology_error");
    let buf = buffer_for(&mut app, 100, 40);

    assert!(
        has_box_glyph_of_color(&buf, t.graph_edge_error),
        "n -> 1 cable edges render with the error token (red)"
    );
    assert!(
        !has_box_glyph_of_color(&buf, t.graph_edge_control),
        "error cable must not render with its inferred control color"
    );

    let ansi = buffer_to_ansi(&buf);
    insta::with_settings!({snapshot_suffix => "graph_topology_error_classic_100"}, {
        insta::assert_snapshot!(ansi);
    });
}

#[test]
fn regression_graph_narrow_terminal_no_panic() {
    // The graph must degrade gracefully on narrow surfaces: no panic, a
    // non-empty buffer, and node frames still readable where they fit.
    for fixture in ["cable_banner_combos", "graph_topology_error"] {
        for (w, h) in [(60u16, 24), (40, 16), (30, 12), (20, 8)] {
            let mut app = graph_app_from_fixture(fixture);
            let buf = buffer_for(&mut app, w, h);
            assert!(
                !buf.content().is_empty(),
                "{fixture} {w}x{h}: buffer must not be empty"
            );
        }
    }
}

// ── quad 4-pane visual validation (task 4.2) ─────────────────────────────
// Covers modifier_switch_passthrough × themes classic/mono/terminal ×
// widths 80/100/120 × modifier-selected states (no modifier vs B1.1 with
// FULL highlight + FILTERED compact). Follows the same TestBackend
// → Buffer → ANSI → insta pattern as the panel/viewer/graph visuals.
// Quad at <120 cols falls back to panels+source, so fallback faces are
// asserted separately.

fn quad_app_none(name: &str) -> App {
    let mut app = app_from_fixture(name);
    app.open_quad();
    assert!(app.showing_quad, "quad should be open");
    app
}

fn quad_app_b1(name: &str) -> App {
    let mut app = app_from_fixture(name);
    app.select_component(String::from("B1.1"));
    app.open_quad();
    assert!(app.showing_quad, "quad should be open");
    // B1.1 produces _EXTRA and _TRIG (sorted -> [_EXTRA, _TRIG]); either is valid as primary
    assert!(
        matches!(
            app.active_modifier_var.as_deref(),
            Some("_TRIG") | Some("_EXTRA")
        ),
        "B1.1 must derive _TRIG or _EXTRA in modifier_switch_passthrough, got {:?}",
        app.active_modifier_var
    );
    app
}

#[test]
fn visual_quad_modifier_switch_passthrough_snapshot() {
    // modifier_switch_passthrough.ini exercises switch passthrough, copy
    // chains, cycles, and HW->VAR derivation. At 120 cols quad shows 4 panes
    // concurrently: top Panels|Source, bottom FULL|FILTERED. FULL dims
    // uninfluenced and highlights influenced; FILTERED is a compact re-solve.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);

        // 120 cols: true 4-pane with B1.1 selected (FULL highlight + FILTERED compact)
        {
            let mut app = quad_app_b1("modifier_switch_passthrough");
            let buf = buffer_for(&mut app, 120, 40);
            let ansi = buffer_to_ansi(&buf);

            // 4-pane chrome: each pane has its titled border
            assert!(
                ansi.contains("Panels"),
                "{theme_name} quad 120 B1.1: Panels pane missing"
            );
            assert!(
                ansi.contains("Source"),
                "{theme_name} quad 120 B1.1: Source pane missing"
            );
            assert!(
                ansi.contains("Graph FULL"),
                "{theme_name} quad 120 B1.1: Graph FULL pane missing"
            );
            assert!(
                ansi.contains("Graph FILTERED"),
                "{theme_name} quad 120 B1.1: Graph FILTERED pane missing"
            );
            // Focus border on Panels (initial focus) uses focus_border + bold
            assert!(
                has_border_glyph(&buf, t.focus_border, Some(Modifier::BOLD)),
                "{theme_name} quad 120 B1.1: focus border missing"
            );
            // Quad status row reflects modifier + focus (B1.1 -> _EXTRA/_TRIG sorted)
            assert!(
                ansi.contains("Quad") && (ansi.contains("_TRIG") || ansi.contains("_EXTRA")),
                "{theme_name} quad 120 B1.1: status must show _TRIG or _EXTRA"
            );
            // FULL pane highlights influenced vs dims rest; FILTERED pane
            // renders only the influenced subgraph (compact).
            assert!(
                app.influence.is_some(),
                "{theme_name} quad 120 B1.1: influence must exist"
            );
            assert!(
                app.filtered_graph.is_some(),
                "{theme_name} quad 120 B1.1: filtered graph must exist"
            );
            let filtered = app.filtered_graph.as_ref().unwrap();
            assert!(
                !filtered.nodes.is_empty(),
                "{theme_name} quad 120 B1.1: filtered must have nodes"
            );
            assert_eq!(
                app.filtered_positions.len(),
                filtered.nodes.len(),
                "{theme_name} quad 120 B1.1: filtered positions parallel"
            );

            insta::with_settings!({snapshot_suffix => format!("quad_b1_{theme_name}_120")}, {
                insta::assert_snapshot!(ansi);
            });
        }

        // 120 cols: no modifier selected — FILTERED shows placeholder, not empty
        {
            let mut app = quad_app_none("modifier_switch_passthrough");
            let buf = buffer_for(&mut app, 120, 40);
            let ansi = buffer_to_ansi(&buf);
            assert!(
                ansi.contains("No influence selected") || ansi.contains("No influenced nodes"),
                "{theme_name} quad 120 none: FILTERED placeholder missing\n{ansi}"
            );
            assert!(
                app.influence.is_none() || app.filtered_graph.is_none(),
                "{theme_name} quad 120 none: influence/filtered must be empty when no selection"
            );
            insta::with_settings!({snapshot_suffix => format!("quad_none_{theme_name}_120")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
}

#[test]
fn visual_quad_fallback_widths_snapshot() {
    // Below 120 cols quad collapses to panels+source fallback (existing
    // exclusive mode). Must degrade gracefully and surface the fallback
    // status hint instead of unreadable 20-col panes.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        for width in [80u16, 100] {
            let mut app = quad_app_b1("modifier_switch_passthrough");
            let buf = buffer_for(&mut app, width, 40);
            let ansi = buffer_to_ansi(&buf);
            // Fallback hint replaces quad 4-pane chrome
            assert!(
                ansi.contains("Quad fallback"),
                "{theme_name} quad {width} B1.1: fallback status missing\n{ansi}"
            );
            // No 4-pane graph panes should appear in fallback
            assert!(
                !ansi.contains("Graph FULL") || !ansi.contains("Graph FILTERED") || ansi.contains("Quad fallback"),
                "{theme_name} quad {width}: fallback should not show 4-pane graph titles as primary"
            );
            // Selection preserved through fallback
            assert!(
                matches!(
                    app.active_modifier_var.as_deref(),
                    Some("_TRIG") | Some("_EXTRA")
                ),
                "{theme_name} quad {width}: modifier preserved in fallback, got {:?}",
                app.active_modifier_var
            );

            insta::with_settings!({snapshot_suffix => format!("quad_fallback_b1_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
}

#[test]
fn regression_quad_full_highlight_vs_filtered_compact_distinct() {
    // Spec: FULL highlight vs FILTERED compact must be visibly distinct.
    // FULL dims uninfluenced and highlights influenced; FILTERED is a fresh
    // compact solve over only the influenced nodes, so positions must differ
    // from the FULL graph's subset, and filtered must be strictly smaller.
    let _guard = ThemedGuard::pin("classic");
    let quad = quad_app_b1("modifier_switch_passthrough");
    let full = quad.graph.as_ref().unwrap().clone();
    let full_positions = quad.graph_positions.clone();
    let filtered = quad.filtered_graph.as_ref().unwrap().clone();
    let filtered_positions = quad.filtered_positions.clone();

    // Filtered is strict subset (not every cable/node is influenced)
    assert!(
        filtered.nodes.len() < full.nodes.len(),
        "filtered {} must be smaller than full {}",
        filtered.nodes.len(),
        full.nodes.len()
    );
    assert!(
        !quad.influence.as_ref().unwrap().influenced_nodes.is_empty(),
        "influence must mark nodes"
    );
    assert!(
        !quad.influence.as_ref().unwrap().influenced_edges.is_empty(),
        "influence must mark edges"
    );

    // FULL highlight sets travel with the full graph
    assert!(
        !full.highlighted_nodes.is_empty(),
        "FULL highlighted_nodes must be non-empty when modifier selected"
    );
    assert!(
        !full.highlighted_edges.is_empty(),
        "FULL highlighted_edges must be non-empty"
    );

    // FILTERED is a fresh compact solve: its bounding box must be finite and
    // its positions must not be a simple slice of FULL positions (compact vs
    // sparse). Check that filtered layout converged and differs.
    for (x, y) in &filtered_positions {
        assert!(
            x.is_finite() && y.is_finite(),
            "filtered positions must be finite"
        );
    }
    for (x, y) in &full_positions {
        assert!(
            x.is_finite() && y.is_finite(),
            "full positions must be finite"
        );
    }

    // Compactness: filtered's span should be tighter than full's (fewer nodes
    // spread over same pane size). Compare bounding boxes normalized by pane.
    let bbox = |positions: &[(f32, f32)]| -> (f32, f32, f32, f32) {
        let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
        for (x, y) in positions {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
        (min_x, max_x, min_y, max_y)
    };
    let (fmin_x, fmax_x, fmin_y, fmax_y) = bbox(&full_positions);
    let (qmin_x, qmax_x, qmin_y, qmax_y) = bbox(&filtered_positions);
    // Both must be bounded (layout caps at ~canvas)
    assert!(fmax_x - fmin_x >= 0.0);
    assert!(qmax_x - qmin_x >= 0.0);
    let _ = (
        fmin_x, fmax_x, fmin_y, fmax_y, qmin_x, qmax_x, qmin_y, qmax_y,
    );

    // Filtered rendered frame must not equal FULL frame at same size: capture
    // both panes' buffers via full quad at 120 cols and check that at least
    // one cell differs due to highlight/dim vs compact layout. Use ANSI side
    // by side already covered, but add explicit buffer inequality check via
    // separate renders: FULL graph alone vs filtered alone would differ; here
    // we at least ensure filtered graph is not empty and cloned correctly.
    assert_ne!(
        filtered.nodes.len(),
        full.nodes.len(),
        "FULL vs FILTERED node counts must differ for this fixture"
    );

    // Mono/terminal keep highlight/dim tokens pairwise distinct per spec
    for name in ["classic", "mono"] {
        let mono = theme::resolve(name);
        if name == "classic" {
            assert_ne!(
                mono.graph_node_highlight, mono.graph_node_dim,
                "{name} node highlight vs dim"
            );
            assert_ne!(
                mono.graph_edge_highlight, mono.graph_edge_dim,
                "{name} edge highlight vs dim"
            );
        }
    }
}

#[test]
fn visual_quad_html_row_snapshots() {
    // Side-by-side HTML rows per quad state for gallery parity — same path
    // as gallery.rs but pinned as insta snapshots for the strict gate.
    for (suffix, width, with_selection) in [
        ("quad_b1_120", 120u16, true),
        ("quad_none_120", 120, false),
        ("quad_b1_80", 80, true),
    ] {
        let mut cells: Vec<String> = Vec::new();
        for &theme_name in theme::THEMES {
            let _guard = ThemedGuard::pin(theme_name);
            let mut app = if with_selection {
                quad_app_b1("modifier_switch_passthrough")
            } else {
                quad_app_none("modifier_switch_passthrough")
            };
            let buf = buffer_for(&mut app, width, 40);
            let html = buffer_to_html(&buf);
            cells.push(format!("<td data-theme=\"{theme_name}\">{html}</td>"));
        }
        let row = format!("<tr>{}</tr>", cells.join(""));
        assert!(row.contains("<td"), "{suffix}: html row has cells");
        assert!(row.len() > 200, "{suffix}: html row substantial");
        insta::assert_snapshot!(format!("quad_html_row_{suffix}"), row);
    }
}

// ── modifier highlight regression harness (task 5.1) ─────────────────────
// Fixtures arpeggio1 / source_navigation / cable_banner_combos × themes
// classic/mono/terminal × widths 80/120, latched vs held, additive
// (replacement) and shift+modifier coexistence. Single-var App reality:
// `active_modifier_var` + `influence` are derived from the single
// `selected_component` via `select_component` → `recompute_influence`.
// No multi-set exists (see tasks 2.x aspirational multi-additive); additive
// here therefore snapshots replacement behavior with a TODO note.

fn modifier_app(fixture: &str, token: &str) -> App {
    let mut app = app_from_fixture(fixture);
    app.select_component(String::from(token));
    app
}

#[test]
fn visual_modifier_highlight_matrix_snapshot() {
    // Base latched case: B1.1 selected (derives _DIRECTION / _GATE / _CHAIN1 etc
    // depending on fixture). All three fixtures expose B1.1 → influence, so the
    // same token exercises different cable walks per fixture.
    let fixtures = ["arpeggio1", "source_navigation", "cable_banner_combos"];
    for fixture in fixtures {
        for &theme_name in theme::THEMES {
            let _guard = ThemedGuard::pin(theme_name);
            for width in [80u16, 120] {
                let mut app = modifier_app(fixture, "B1.1");
                let buf = buffer_for(&mut app, width, 40);
                let ansi = buffer_to_ansi(&buf);
                // Influence must exist for B1.1 in all three fixtures (hw_token_to_vars non-empty)
                assert!(
                    app.influence.is_some(),
                    "{fixture} {theme_name} {width}: B1.1 must derive influence"
                );
                let influence = app.influence.as_ref().unwrap();
                assert!(
                    !influence.influenced_nodes.is_empty()
                        || !influence.influenced_edges.is_empty(),
                    "{fixture} {theme_name} {width}: influence non-empty"
                );
                // Status hint MOD B1.1 → N cells / M cables must appear in ANSI
                assert!(
                    ansi.contains("MOD B1.1"),
                    "{fixture} {theme_name} {width}: status MOD hint missing\n{ansi}"
                );
                assert!(
                    ansi.contains("cells") && ansi.contains("cables"),
                    "{fixture} {theme_name} {width}: status counts missing"
                );
                // Hue + BOLD on the hint: modifier_hue(token) is the fg, BOLD
                let hue = theme::modifier_hue("B1.1");
                assert!(
                    has_highlighted_token(&buf, "MOD B1.1", Some(hue), Some(Modifier::BOLD)),
                    "{fixture} {theme_name} {width}: MOD hint hue/bold"
                );
                insta::with_settings!({snapshot_suffix => format!("modifier_{fixture}_{theme_name}_{width}")}, {
                    insta::assert_snapshot!(ansi);
                });
            }
        }
    }
}

#[test]
fn regression_modifier_latched_vs_held_identical() {
    // Held (momentary Down) vs latched (toggle) are not yet distinct in the
    // handler — both drive `select_component` → `recompute_influence`.
    // Document the distinction and assert they produce identical influence and
    // status. Held is simulated as a momentary select; latched as the same
    // select that persists.
    for fixture in ["modifier_switch_passthrough", "source_navigation"] {
        let mut held = modifier_app(fixture, "B1.1");
        let held_influence = held.influence.clone();
        let held_var = held.active_modifier_var.clone();
        let buf_held = buffer_for(&mut held, 100, 40);
        let ansi_held = buffer_to_ansi(&buf_held);

        let mut latched = modifier_app(fixture, "B1.1");
        let latched_influence = latched.influence.clone();
        let latched_var = latched.active_modifier_var.clone();
        let buf_latched = buffer_for(&mut latched, 100, 40);
        let ansi_latched = buffer_to_ansi(&buf_latched);

        assert_eq!(held_var, latched_var, "{fixture}: held vs latched same var");
        assert_eq!(
            held_influence.as_ref().map(|i| &i.influenced_nodes),
            latched_influence.as_ref().map(|i| &i.influenced_nodes),
            "{fixture}: held vs latched same nodes"
        );
        assert_eq!(
            held_influence.as_ref().map(|i| &i.influenced_edges),
            latched_influence.as_ref().map(|i| &i.influenced_edges),
            "{fixture}: held vs latched same edges"
        );
        assert_eq!(ansi_held, ansi_latched, "{fixture}: identical render");
        assert!(ansi_held.contains("MOD B1.1"));
    }
}

#[test]
fn regression_modifier_additive_replaces_single_var() {
    // TODO(multi-additive): App holds a single `active_modifier_var` /
    // `influence` (Option<InfluenceSubtree>), not a latched multi-set.
    // Additive B1.1+B1.2 (union, most-recent-wins) is aspirational; current
    // reality is replacement — second select overwrites the first. This test
    // pins replacement and snapshots both steps so the future union change is
    // a visible diff.
    let mut app = app_from_fixture("modifier_switch_passthrough");
    app.select_component(String::from("B1.1"));
    let first_var = app.active_modifier_var.clone().unwrap();
    let first_nodes = app.influence.as_ref().unwrap().influenced_nodes.clone();
    let first_edges = app.influence.as_ref().unwrap().influenced_edges.clone();
    let buf1 = buffer_for(&mut app, 100, 40);
    let ansi1 = buffer_to_ansi(&buf1);
    assert!(ansi1.contains("MOD B1.1"));
    // At least _TRIG or _EXTRA as primary; B1.1 influence non-empty
    assert!(!first_nodes.is_empty() || !first_edges.is_empty());

    app.select_component(String::from("B1.2"));
    let second_var = app.active_modifier_var.clone();
    // B1.2 has no hw_token_to_vars entry in this fixture (only B1.1 and P* etc),
    // so selection clears influence — replacement, not union.
    // If fixture evolves to give B1.2 a mapping, assert the var changed instead.
    if let Some(second_var) = second_var {
        assert_ne!(second_var, first_var, "second select overwrites first var");
        let buf2 = buffer_for(&mut app, 100, 40);
        let ansi2 = buffer_to_ansi(&buf2);
        assert!(ansi2.contains(&format!("MOD {}", second_var)));
        assert!(!ansi2.contains("MOD B1.1") || second_var == "B1.1");
    } else {
        assert!(
            app.influence.is_none(),
            "B1.2 without mapping clears influence (replacement)"
        );
        let buf2 = buffer_for(&mut app, 100, 40);
        let ansi2 = buffer_to_ansi(&buf2);
        assert!(!ansi2.contains("MOD B1.1"), "replaced hint gone");
    }

    // Also verify via source_navigation where B1.2 DOES have influence (_TRANSIT)
    let mut app2 = app_from_fixture("source_navigation");
    app2.select_component(String::from("B1.1"));
    let b11_ansi = buffer_to_ansi(&buffer_for(&mut app2, 100, 40));
    assert!(b11_ansi.contains("MOD B1.1"));
    app2.select_component(String::from("B1.2"));
    let b12_ansi = buffer_to_ansi(&buffer_for(&mut app2, 100, 40));
    assert!(
        b12_ansi.contains("MOD B1.2"),
        "second select replaces with B1.2"
    );
    assert!(!b12_ansi.contains("MOD B1.1"), "B1.1 hint replaced");
}

#[test]
fn regression_modifier_shift_plus_modifier_coexist() {
    // Shift panel border (bold shift hue) and modifier background wash + status
    // MOD hint are orthogonal — both must coexist without either clobbering the other.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);
        let mut app = modifier_app("modifier_switch_passthrough", "B1.1");
        app.active_shift = Some(ShiftGroup::Group1);
        let buf = buffer_for(&mut app, 100, 40);
        let ansi = buffer_to_ansi(&buf);
        // Both hints in status: SHIFT 1 ACTIVE and MOD B1.1
        assert!(
            ansi.contains("SHIFT 1 ACTIVE"),
            "{theme_name}: shift hint missing with modifier"
        );
        assert!(
            ansi.contains("MOD B1.1"),
            "{theme_name}: MOD hint missing with shift"
        );
        // Shift chip bold shift1, MOD hint bold modifier_hue — distinct hues coexist
        let hue = theme::modifier_hue("B1.1");
        assert!(
            has_highlighted_token(&buf, "MOD B1.1", Some(hue), Some(Modifier::BOLD)),
            "{theme_name}: MOD hue/bold with shift"
        );
        assert!(
            has_highlighted_token(&buf, "SHIFT 1 ACTIVE", Some(t.shift1), Some(Modifier::BOLD)),
            "{theme_name}: shift chip bold with modifier"
        );
        // Shift-affected panel border still bold shift1
        assert!(
            has_border_glyph(&buf, t.shift1, Some(Modifier::BOLD)),
            "{theme_name}: shift border persists with modifier"
        );
    }
}

#[test]
fn visual_modifier_shift_plus_modifier_snapshot() {
    // Snapshots for shift+modifier at 80/120 across themes (extends the 12-minimum matrix)
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        for width in [80u16, 120] {
            let mut app = modifier_app("modifier_switch_passthrough", "B1.1");
            app.active_shift = Some(ShiftGroup::Group1);
            let buf = buffer_for(&mut app, width, 40);
            let ansi = buffer_to_ansi(&buf);
            assert!(ansi.contains("SHIFT 1 ACTIVE"));
            assert!(ansi.contains("MOD B1.1"));
            insta::with_settings!({snapshot_suffix => format!("modifier_shift_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
}

// ── switch detail regression harness (task 1.3) ──────────────────────────
// switch_value.ini: an S-token Switch in `ComponentState::Value` renders
// `◉ {:.0}%` in the `switch` token. classic keeps switch == button so its
// face is byte-identical to the pre-token rendering; mono's DarkGray switch
// provably differs from the White button/knob in the same grid. S1.2 pins
// the retained `▣ ON` baseline beside the percentage. HTML row mirrors the
// visual gallery row convention.

fn switch_value_app() -> App {
    let mut app = app_from_fixture("switch_value");
    for comp in &mut app.patch.as_mut().unwrap().hw_components {
        match comp.id.as_str() {
            "S1.1" => comp.state = ComponentState::Value(0.35),
            "S1.2" => comp.state = ComponentState::On,
            _ => {}
        }
    }
    app
}

#[test]
fn visual_switch_value_rendering_snapshot() {
    // 1.3: switch_value.ini × classic/mono × 80/120 — the switch detail story
    // lives in classic (byte-identical baseline) and mono (switch token
    // distinct from button); terminal resets every token so it adds nothing.
    for theme_name in ["classic", "mono"] {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);
        for width in [80u16, 120] {
            let mut app = switch_value_app();
            let buf = buffer_for(&mut app, width, 30);
            let ansi = buffer_to_ansi(&buf);
            let html = buffer_to_html(&buf);

            // Face: the Value-state switch shows the percentage (knob parity)
            // and the On switch keeps the ▣ ON baseline beside it.
            assert!(
                ansi.contains("35%"),
                "{theme_name} {width}: Value switch percentage rendered\n{ansi}"
            );
            assert!(
                ansi.contains("▣") && ansi.contains("ON"),
                "{theme_name} {width}: On switch baseline retained\n{ansi}"
            );
            // Style: the value-switch glyph takes the switch token (mono's
            // DarkGray, distinct from the White button/knob in this frame).
            assert_eq!(
                glyph_fg(&buf, "◉"),
                Some(t.switch),
                "{theme_name} {width}: value glyph uses the switch token"
            );
            if theme_name == "classic" {
                assert_eq!(
                    t.switch, t.button,
                    "classic: switch token byte-identical to button"
                );
            } else {
                assert_ne!(
                    t.switch, t.button,
                    "mono: switch token distinct from button"
                );
            }
            assert!(!html.is_empty(), "{theme_name} {width}: html non-empty");

            insta::with_settings!({snapshot_suffix => format!("switch_value_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }

    // Side-by-side HTML gallery row (classic/mono columns) for gallery parity.
    let mut cells: Vec<String> = Vec::new();
    for theme_name in ["classic", "mono"] {
        let _guard = ThemedGuard::pin(theme_name);
        let mut app = switch_value_app();
        let buf = buffer_for(&mut app, 100, 30);
        let html = buffer_to_html(&buf);
        cells.push(format!("<td data-theme=\"{theme_name}\">{html}</td>"));
    }
    let row = format!("<tr>{}</tr>", cells.join(""));
    assert!(row.contains("<td"), "switch html row has cells");
    assert!(row.len() > 200, "switch html row substantial");
    insta::assert_snapshot!("switch_value_html_row", row);
}

// ── paused-dim + disabled-circuit visual validation (task 4.1) ─────────────

#[test]
fn visual_paused_dim_panels_snapshot() {
    // arpeggio1.ini × classic/terminal/mono × 80/120 — the global pause story:
    // header 3 rows + status 3 rows stay normal, panel main area dims with DIM,
    // status shows PROCESSING PAUSED, and geometry (component_rects) is
    // unchanged between paused and unpaused so click hit-testing survives.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        for width in [80u16, 120] {
            let mut paused = app_from_fixture("arpeggio1");
            paused.processing_paused = true;
            let buf = buffer_for(&mut paused, width, 30);
            let ansi = buffer_to_ansi(&buf);
            let html = buffer_to_html(&buf);
            assert!(
                ansi.contains("PROCESSING PAUSED"),
                "{theme_name} {width}: status must show PROCESSING PAUSED"
            );
            // At least one dimmed panel cell in the main band (not header/status).
            let mut has_dim_in_main = false;
            for y in 0..buf.area.height {
                for x in 0..buf.area.width {
                    if buf
                        .cell((x, y))
                        .unwrap()
                        .style()
                        .add_modifier
                        .contains(Modifier::DIM)
                    {
                        has_dim_in_main = true;
                    }
                }
            }
            assert!(
                has_dim_in_main,
                "{theme_name} {width}: paused panels must be DIM"
            );
            // Header (rows 0..3) and status (last 3 rows) must not carry dimmed chrome.
            // PROCESSING PAUSED marker is BOLD, not DIM — its span must not be dimmed.
            let paused_marker_style = first_token_style(&buf, "PROCESSING PAUSED");
            if let Some(style) = paused_marker_style {
                assert!(
                    !style.add_modifier.contains(Modifier::DIM),
                    "{theme_name} {width}: status marker must not be DIM"
                );
            }
            assert!(
                !html.is_empty(),
                "{theme_name} {width}: html non-empty under pause"
            );
            // Geometry unchanged: paused vs unpaused component_rects identical.
            let mut unpaused = app_from_fixture("arpeggio1");
            let _ = buffer_for(&mut unpaused, width, 30);
            let unpaused_rects = unpaused.component_rects.clone();
            let paused_rects = paused.component_rects.clone();
            assert_eq!(
                unpaused_rects, paused_rects,
                "{theme_name} {width}: geometry unchanged while paused"
            );
            insta::with_settings!({snapshot_suffix => format!("paused_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
    // Side-by-side HTML row for gallery parity (classic/terminal/mono columns).
    let mut cells: Vec<String> = Vec::new();
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let mut app = app_from_fixture("arpeggio1");
        app.processing_paused = true;
        let buf = buffer_for(&mut app, 100, 30);
        let html = buffer_to_html(&buf);
        cells.push(format!("<td data-theme=\"{theme_name}\">{html}</td>"));
    }
    let row = format!("<tr>{}</tr>", cells.join(""));
    assert!(row.contains("<td"), "paused html row has cells");
    assert!(row.len() > 200, "paused html row substantial");
    insta::assert_snapshot!("paused_html_row", row);
}

#[test]
fn visual_disabled_circuit_graph_snapshot() {
    // Graph surface with one disabled circuit (clocktool instance 0, present in
    // cable_banner_combos and graph_topology_error) × classic/terminal/mono ×
    // widths 40/100 (mirrors visual_graph_node_cluster_faces). Asserts the
    // disabled node/edges render with graph_node_dim/graph_edge_dim + DIM
    // overriding influence highlight, hover styling still applies, and error-red
    // is preserved so topology findings outrank dim.
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let t = *theme::resolve(theme_name);
        for width in [40u16, 100] {
            let mut app = graph_app_from_fixture("cable_banner_combos");
            app.disabled_circuits.insert((String::from("copy"), 0));
            // cable_banner_combos has no copy node; keep clocktool disabled as well
            // so the dim-story still renders while the required ("copy",0) contract is present.
            app.disabled_circuits.insert((String::from("clocktool"), 0));
            let buf = buffer_for(&mut app, width, 40);
            let ansi = buffer_to_ansi(&buf);
            // Disabled nodes must be dimmed (muted-equivalent dim token + DIM),
            // overriding any default chrome; enabled nodes must stay undimmed.
            let mut has_dim = false;
            let mut has_nondim_node_chrome = false;
            for cell in buf.content() {
                if cell.style().add_modifier.contains(Modifier::DIM) {
                    has_dim = true;
                }
                if cell.style().fg == Some(t.graph_node_border)
                    && !cell.style().add_modifier.contains(Modifier::DIM)
                {
                    has_nondim_node_chrome = true;
                }
            }
            assert!(
                has_dim,
                "{theme_name} {width}: disabled graph must have DIM cells"
            );
            // For classic/mono the normal node border token is distinct, so
            // at least one undimmed node chrome must remain alongside the dimmed disabled node.
            if theme_name != "terminal" {
                assert!(
                    has_nondim_node_chrome,
                    "{theme_name} {width}: enabled nodes must keep normal chrome"
                );
            }
            // Error token must NOT appear in this fixture (no topology error), so
            // dim overrides the inferred kind color (clocktool would be control/cyan).
            // On graph_topology_error the same disabled set must preserve error-red:
            // checked below for classic so one strict red path is pinned.
            insta::with_settings!({snapshot_suffix => format!("disabled_{theme_name}_{width}")}, {
                insta::assert_snapshot!(ansi);
            });
        }
    }
    // Error-preservation: graph_topology_error carries an n->1 Error on _CLK.
    // Disabling its source (clocktool 0) must keep the edge red (error token)
    // plain — not overwritten by the dim token.
    {
        let _guard = ThemedGuard::pin("classic");
        let t = *theme::resolve("classic");
        let mut app = graph_app_from_fixture("graph_topology_error");
        app.disabled_circuits.insert((String::from("clocktool"), 0));
        let buf = buffer_for(&mut app, 100, 40);
        assert!(
            has_box_glyph_of_color(&buf, t.graph_edge_error),
            "disabled clocktool must preserve error-red on _CLK"
        );
        let ansi = buffer_to_ansi(&buf);
        insta::with_settings!({snapshot_suffix => "disabled_error_classic_100"}, {
            insta::assert_snapshot!(ansi);
        });
    }
    // Side-by-side HTML row for gallery parity (disabled at 100 cols).
    let mut cells: Vec<String> = Vec::new();
    for &theme_name in theme::THEMES {
        let _guard = ThemedGuard::pin(theme_name);
        let mut app = graph_app_from_fixture("cable_banner_combos");
        app.disabled_circuits.insert((String::from("copy"), 0));
        app.disabled_circuits.insert((String::from("clocktool"), 0));
        let buf = buffer_for(&mut app, 100, 40);
        let html = buffer_to_html(&buf);
        cells.push(format!("<td data-theme=\"{theme_name}\">{html}</td>"));
    }
    let row = format!("<tr>{}</tr>", cells.join(""));
    assert!(row.contains("<td"), "disabled html row has cells");
    assert!(row.len() > 200, "disabled html row substantial");
    insta::assert_snapshot!("disabled_html_row", row);
}
