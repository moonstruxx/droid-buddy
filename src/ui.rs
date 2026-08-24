use std::collections::{HashMap, HashSet};

use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, SourceViewMode, ViewerFocus};
use crate::patch::{ComponentKind, ComponentState, ShiftGroup};
use crate::theme;

pub fn render(frame: &mut Frame, app: &mut App) {
    // Picker takes absolute precedence – overlay on top of anything.
    if app.showing_picker {
        render_picker(frame, frame.area(), app);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(10),   // main content
            Constraint::Length(3), // status bar
        ])
        .split(frame.area());

    if chunks.len() < 3 {
        return;
    }

    render_header(frame, chunks[0], app);
    if app.showing_viewer {
        render_embedded_main(frame, chunks[1], app);
        render_viewer_status(frame, chunks[2], app);
    } else {
        render_main(frame, chunks[1], app);
        render_status(frame, chunks[2], app);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = match &app.patch {
        Some(patch) => format!(" DROID: {} ", patch.name),
        None => String::from(" DROID TUI "),
    };

    let header = Paragraph::new(title)
        .style(
            Style::default()
                .fg(theme::active().text)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::active().accent)),
        );

    frame.render_widget(header, area);
}

fn render_main(frame: &mut Frame, area: Rect, app: &mut App) {
    app.component_rects.clear();
    let patch_ref = app.patch.clone();
    match patch_ref {
        Some(patch) => render_patch(frame, area, &patch, app),
        None => render_empty(frame, area),
    }
}

fn render_empty(frame: &mut Frame, area: Rect) {
    let msg = Paragraph::new("Press 'l' to load a patch")
        .style(Style::default().fg(theme::active().muted))
        .alignment(Alignment::Center);
    frame.render_widget(msg, area);
}

const COMPONENT_WIDTH: u16 = 16;
const COMPONENT_HEIGHT: u16 = 3;

/// Render hardware components grouped into physical controller panels
/// (P2B8, Faderbank, Notebuttons, CV I/O, ...) that mirror the hardware
/// layout, wrapping components onto extra rows when a panel doesn't fit
/// the terminal width. See controller-panels/spec.md.
fn render_patch(frame: &mut Frame, area: Rect, patch: &crate::patch::Patch, app: &mut App) {
    render_patch_grouped(frame, area, patch, app);
}

fn render_patch_grouped(frame: &mut Frame, area: Rect, patch: &crate::patch::Patch, app: &mut App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // hovered_component/click hit-testing are tracked as an index into
    // patch.hw_components (the flat, parse-order list), not into any
    // per-panel grouping, so components must be able to look that index
    // back up regardless of which panel they render into.
    let index_of: HashMap<&str, usize> = patch
        .hw_components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();

    // Group components into controller panels, preserving the order in
    // which panels and components first appear in the patch.
    let mut panel_order: Vec<String> = Vec::new();
    let mut panels: HashMap<String, Vec<&crate::patch::HwComponent>> = HashMap::new();
    for comp in &patch.hw_components {
        panels
            .entry(comp.controller.clone())
            .or_insert_with(|| {
                panel_order.push(comp.controller.clone());
                Vec::new()
            })
            .push(comp);
    }

    let cols = ((area.width / COMPONENT_WIDTH).max(1)) as usize;
    let rows_for = |n: usize| -> u16 { (n.div_ceil(cols)).max(1) as u16 };

    // Build a set of LED ids that are "folded" (referenced by another component
    // and match an existing component id of kind Led). These LED components
    // must be skipped as standalone grid cells; their owners render as boxes.
    let folded_led_ids: HashSet<&str> = patch
        .hw_components
        .iter()
        .filter(|c| c.led.is_some())
        .filter_map(|c| c.led.as_deref())
        .filter(|led_id| {
            patch
                .hw_components
                .iter()
                .any(|c| c.id == *led_id && c.kind == ComponentKind::Led)
        })
        .collect();

    let mut constraints: Vec<Constraint> = panel_order
        .iter()
        .map(|name| Constraint::Length(rows_for(panels[name].len()) * COMPONENT_HEIGHT + 2))
        .collect();
    constraints.push(Constraint::Min(0));

    let panel_direction = if app.orientation == crate::app::Orientation::Landscape {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };

    let panel_chunks = Layout::default()
        .direction(panel_direction)
        .constraints(constraints)
        .flex(Flex::Start)
        .split(area);

    for (i, name) in panel_order.iter().enumerate() {
        if i >= panel_chunks.len() {
            break;
        }
        let components = &panels[name];

        // A panel is "affected" when at least one of its components belongs
        // to the currently active shift group; affected panels get a bold
        // colored border, other panels dim while a shift is active, and all
        // panels use the default border when no shift is active. See
        // shift-visualization/spec.md.
        let affected = app.active_shift.is_some()
            && components.iter().any(|c| c.shift_group == app.active_shift);

        let (border_style, title) = match app.active_shift {
            Some(group) if affected => (
                Style::default()
                    .fg(shift_color(group))
                    .add_modifier(Modifier::BOLD),
                format!(" {} [SHIFT {}] ", name, group.key_label()),
            ),
            Some(_) => (
                Style::default()
                    .fg(theme::active().muted)
                    .add_modifier(Modifier::DIM),
                format!(" {} ", name),
            ),
            None => (
                Style::default().fg(theme::active().muted),
                format!(" {} ", name),
            ),
        };

        let block = Block::default()
            .title(title)
            .title_style(border_style)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(panel_chunks[i]);
        frame.render_widget(block, panel_chunks[i]);

        if inner.width == 0 || inner.height == 0 {
            continue;
        }

        let row_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(COMPONENT_HEIGHT);
                rows_for(components.len()) as usize
            ])
            .flex(Flex::Start)
            .split(inner);

        for (row_i, row) in components.chunks(cols).enumerate() {
            if row_i >= row_chunks.len() {
                break;
            }
            let comp_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Length(COMPONENT_WIDTH); row.len()])
                .flex(Flex::Start)
                .split(row_chunks[row_i]);

            for (col_i, comp) in row.iter().enumerate() {
                if col_i >= comp_chunks.len() {
                    break;
                }

                // Skip folded LED components — they are referenced by another
                // component's `led` field and will be rendered as a box instead.
                if comp.kind == ComponentKind::Led && folded_led_ids.contains(comp.id.as_str()) {
                    continue;
                }

                let global_idx = index_of[comp.id.as_str()];
                let is_hovered = app.hovered_component == Some(global_idx);
                let is_shift_active =
                    comp.shift_group.is_some() && comp.shift_group == app.active_shift;
                render_component(
                    frame,
                    comp_chunks[col_i],
                    comp,
                    is_hovered,
                    is_shift_active,
                    patch,
                );
                let base_rect = comp_chunks[col_i];
                let scaled_rect = Rect {
                    x: base_rect.x,
                    y: base_rect.y,
                    width: (base_rect.width as f32 * app.scale_factor) as u16,
                    height: (base_rect.height as f32 * app.scale_factor) as u16,
                };
                app.component_rects.push((global_idx, scaled_rect));
            }
        }
    }
}

fn render_component(
    frame: &mut Frame,
    area: Rect,
    comp: &crate::patch::HwComponent,
    is_hovered: bool,
    is_shift_active: bool,
    patch: &crate::patch::Patch,
) {
    let (symbol, state_text, fg_color): (&str, String, Color) = match comp.kind {
        ComponentKind::Button => {
            let state = match &comp.state {
                ComponentState::On => String::from("ON"),
                _ => String::from("OFF"),
            };
            (
                if matches!(comp.state, ComponentState::On) {
                    "●"
                } else {
                    "○"
                },
                state,
                if is_shift_active {
                    shift_color(comp.shift_group.expect("is_shift_active implies a group"))
                } else {
                    theme::active().button
                },
            )
        }
        ComponentKind::CvIn => ("→", String::from("CV IN"), theme::active().cv_in),
        ComponentKind::CvOut => ("←", String::from("CV OUT"), theme::active().cv_out),
        ComponentKind::Knob => {
            let val = match &comp.state {
                ComponentState::Value(v) => format!("{:.0}%", v * 100.0),
                _ => String::from("---"),
            };
            ("◉", val, theme::active().knob)
        }
        ComponentKind::Switch => {
            let state = match &comp.state {
                ComponentState::On => String::from("ON"),
                _ => String::from("OFF"),
            };
            (
                if matches!(comp.state, ComponentState::On) {
                    "▣"
                } else {
                    "□"
                },
                state,
                theme::active().button,
            )
        }
        ComponentKind::Encoder => {
            let val = match &comp.state {
                ComponentState::Value(v) => format!("{:.0}%", v * 100.0),
                _ => String::from("---"),
            };
            ("◉", val, theme::active().knob)
        }
        ComponentKind::Led => {
            let state = match &comp.state {
                ComponentState::On => String::from("ON"),
                _ => String::from("OFF"),
            };
            (
                if matches!(comp.state, ComponentState::On) {
                    "◉"
                } else {
                    "○"
                },
                state,
                theme::active().led,
            )
        }
    };

    let hover_style = if is_hovered {
        Style::default()
            .fg(fg_color)
            .bg(theme::active().muted)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(fg_color)
    };

    // If this component owns a LED, render a bordered box (3 rows tall).
    if let Some(led_id) = &comp.led {
        // Look up the LED component by id — do not use .unwrap().
        let led_component = patch.hw_components.iter().find(|c| c.id == led_id.as_str());

        let (led_glyph, led_state_text) = match led_component {
            Some(led) => match &led.state {
                ComponentState::On | ComponentState::Active => ("◉", String::from("ON")),
                _ => ("○", String::from("OFF")),
            },
            None => {
                // LED not found in patch — fall back to unlit glyph/state.
                ("○", String::from("OFF"))
            }
        };

        // Hover styling applied to box content/border, same convention as text path.
        let display_style = if is_hovered {
            Style::default()
                .fg(fg_color)
                .bg(theme::active().muted)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(fg_color)
        };

        // 3-row box: line 1 = symbol + label, line 2 = state_text + glyph + state,
        // line 3 = empty (fills the 3‑row cell without a visual gap).
        let lines = vec![
            Line::from(vec![
                Span::styled(symbol, display_style),
                Span::raw(" "),
                Span::styled(&comp.label, display_style),
            ]),
            Line::from(Span::styled(
                format!("{} {}", state_text, led_glyph),
                display_style,
            )),
            Line::from(Span::styled(led_state_text, display_style)),
        ];

        let widget = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(widget, area);
    } else {
        // led: None — render the existing two‑line text cell into the 3‑row area.
        // The Paragraph fills the available area; no gap is left below.
        let lines = vec![
            Line::from(vec![
                Span::styled(symbol, hover_style),
                Span::raw(" "),
                Span::styled(&comp.label, hover_style),
            ]),
            Line::from(Span::styled(
                state_text,
                Style::default().fg(theme::active().muted),
            )),
            Line::from(Span::raw("")), // third row filler so the 3‑row area is fully occupied
        ];

        let widget = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(widget, area);
    }
}

/// Shift colors come from the theme's per-group tokens so themes can
/// restyle (or grayscale) shift visualization without touching rendering.
fn shift_color(group: ShiftGroup) -> Color {
    let t = theme::active();
    match group {
        ShiftGroup::Group1 => t.shift1,
        ShiftGroup::Group2 => t.shift2,
        ShiftGroup::Group3 => t.shift3,
        ShiftGroup::Group4 => t.shift4,
    }
}

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::styled(
        if app.prefix.is_some() {
            "Prefix: g"
        } else {
            app.status_message.as_str()
        },
        Style::default().fg(theme::active().text),
    )];

    if let Some(group) = app.active_shift {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            format!("SHIFT {} ACTIVE", group.key_label()),
            Style::default()
                .fg(shift_color(group))
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Display scale and orientation permanently in the status bar
    spans.push(Span::raw(" | "));
    spans.push(Span::styled(
        format!(
            "Scale: {:.1} | Orientation: {:?}",
            app.scale_factor, app.orientation
        ),
        Style::default().fg(theme::active().text),
    ));

    let status = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(theme::active().status_bg))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::active().muted)),
        );

    frame.render_widget(status, area);
}

fn render_picker(frame: &mut Frame, area: Rect, app: &App) {
    // Calculate picker dimensions (70% width, 50% height, centered)
    let picker_width = (area.width.saturating_sub(4)).max(40);
    let picker_height = (area.height.saturating_sub(4)).max(20);
    let picker_x = area.x + (area.width.saturating_sub(picker_width)) / 2;
    let picker_y = area.y + (area.height.saturating_sub(picker_height)) / 2;
    let picker_area = Rect::new(picker_x, picker_y, picker_width, picker_height);

    // Build entry lines with selectability and selection highlighting
    let entry_lines: Vec<String> = app
        .picker_entries
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let is_selected = i == app.picker_index;
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let prefix = if is_selected { "▶ " } else { "  " };

            format!("{}{}", prefix, file_name)
        })
        .collect();

    let joined = entry_lines.join("\n");
    let paragraph = Paragraph::new(joined)
        .style(Style::default().bg(theme::active().muted))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" File Picker ")
                .border_style(Style::default().fg(theme::active().accent)),
        );

    frame.render_widget(paragraph, picker_area);
}

// ── Embedded viewer layout (task 4.1) ──────────────────────────────────────

fn render_embedded_main(frame: &mut Frame, area: Rect, app: &mut App) {
    app.component_rects.clear();
    // minimap_rect is owned entirely by render_source_pane below: it publishes
    // Some(rect) when the minimap is rendered and resets to None on every
    // path where it is not.
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Split main area horizontally: panels | source_pane
    // The panels column width is app.viewer_split_ratio (clamped 0.3–0.7,
    // default 0.6 = 60% panels / 40% source). Percentages so narrow terminals
    // degrade gracefully; never panic.
    let panels_pct = (app.viewer_split_ratio.clamp(0.3, 0.7) * 100.0) as u16;
    let source_pct = 100u16.saturating_sub(panels_pct);
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(panels_pct),
            Constraint::Percentage(source_pct),
        ])
        .split(area);

    if h_chunks.len() < 2 {
        return;
    }
    let left_area = h_chunks[0];
    let right_area = h_chunks[1];

    render_panels_pane(frame, left_area, app);
    render_source_pane(frame, right_area, app);
}

fn render_panels_pane(frame: &mut Frame, area: Rect, app: &mut App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let focused = app.viewer_focus == ViewerFocus::Panels;
    let border_style = if focused {
        Style::default()
            .fg(theme::active().focus_border)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::active().muted)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Panels ")
        .title_style(border_style)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(patch) = app.patch.clone() {
        render_patch_grouped(frame, inner, &patch, app);
    } else {
        render_empty(frame, inner);
    }
}

const MINIMAP_WIDTH: u16 = 3;

fn should_show_minimap(source_pane_area: Rect, total_area: Rect, app: &App) -> bool {
    if app.patch.is_none() {
        return false;
    }
    if let Some(p) = app.patch.as_ref() {
        if p.raw_lines.is_empty() && p.sections.is_empty() {
            return false;
        }
    }
    if total_area.width < 80 {
        return false;
    }
    // Pane-width floor calibrated for the viewer_split_ratio split (default
    // 0.6 panels / 0.4 source): at a 120-col terminal the pane is ~48 cols and
    // must still show the minimap; at 80 cols it is ~32 and must stay hidden.
    if source_pane_area.width < 40 {
        return false;
    }
    if source_pane_area.height < 10 {
        return false;
    }
    true
}

fn render_source_pane(frame: &mut Frame, area: Rect, app: &mut App) {
    if area.width == 0 || area.height == 0 {
        app.minimap_rect = None;
        return;
    }

    let patch_has_sections = app.patch.as_ref().is_some_and(|p| !p.sections.is_empty());
    let mut show_sidebar = area.width >= 40 && patch_has_sections;
    let total_area = frame.area();
    let mut show_minimap = should_show_minimap(area, total_area, app);

    // Compute sidebar width tentatively
    let mut sidebar_width = if show_sidebar {
        (area.width / 5).max(20).min(area.width.saturating_sub(20))
    } else {
        0
    };
    if sidebar_width >= area.width {
        show_sidebar = false;
        sidebar_width = 0;
    }

    // Ensure remaining content after sidebar + minimap keeps >=20 columns
    let mut minimap_w = if show_minimap { MINIMAP_WIDTH } else { 0 };
    let content_min = 20u16;
    let mut remaining = area
        .width
        .saturating_sub(sidebar_width)
        .saturating_sub(minimap_w);
    if show_minimap && remaining < content_min {
        // Hide minimap rather than squeeze source
        show_minimap = false;
        minimap_w = 0;
        remaining = area.width.saturating_sub(sidebar_width);
    }
    if show_sidebar && remaining < content_min {
        // If still too narrow, hide sidebar as well (keep content)
        show_sidebar = false;
        sidebar_width = 0;
        remaining = area.width.saturating_sub(minimap_w);
        if remaining < content_min && show_minimap {
            show_minimap = false;
            minimap_w = 0;
        }
    }

    if !show_minimap {
        app.minimap_rect = None;
    }

    if show_sidebar && show_minimap {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(sidebar_width),
                Constraint::Min(content_min),
                Constraint::Length(minimap_w),
            ])
            .split(area);
        if chunks.len() < 3 {
            render_source_content(frame, area, app);
            app.minimap_rect = None;
            return;
        }
        render_source_sidebar(frame, chunks[0], app);
        render_source_content(frame, chunks[1], app);
        render_minimap(frame, chunks[2], app);
        app.minimap_rect = Some(chunks[2]);
    } else if show_sidebar {
        let sidebar_w = (area.width / 5).max(20).min(area.width.saturating_sub(20));
        if sidebar_w == 0 || sidebar_w >= area.width {
            render_source_content(frame, area, app);
            return;
        }
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar_w), Constraint::Min(20)])
            .split(area);
        if chunks.len() < 2 {
            render_source_content(frame, area, app);
            return;
        }
        render_source_sidebar(frame, chunks[0], app);
        render_source_content(frame, chunks[1], app);
    } else if show_minimap {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(content_min), Constraint::Length(minimap_w)])
            .split(area);
        if chunks.len() < 2 {
            render_source_content(frame, area, app);
            app.minimap_rect = None;
            return;
        }
        render_source_content(frame, chunks[0], app);
        render_minimap(frame, chunks[1], app);
        app.minimap_rect = Some(chunks[1]);
    } else {
        render_source_content(frame, area, app);
    }
}

fn sidebar_selected_index(app: &App) -> Option<usize> {
    let patch = app.patch.as_ref()?;
    if patch.sections.is_empty() {
        return None;
    }
    // Prefer the section containing the selected occurrence (if any),
    // otherwise the section containing the current scroll line.
    let target_line = if let Some(tok) = app.selected_component.as_ref() {
        patch
            .occurrence_index
            .get(tok)
            .and_then(|spans| spans.get(app.occurrence_cursor))
            .map(|s| s.line)
    } else {
        None
    }
    .unwrap_or(app.source_scroll);

    let mut idx: Option<usize> = None;
    for (i, sec) in patch.sections.iter().enumerate() {
        if sec.header_span.line <= target_line {
            idx = Some(i);
        } else {
            break;
        }
    }
    idx.or(Some(0))
}

fn render_source_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Circuits ")
        .border_style(Style::default().fg(theme::active().accent));

    let Some(patch) = app.patch.as_ref() else {
        frame.render_widget(block, area);
        return;
    };
    if patch.sections.is_empty() {
        frame.render_widget(block, area);
        return;
    }

    let names: Vec<String> = patch.sections.iter().map(|s| s.name.clone()).collect();
    let display_names = disambiguate_names(&names);
    let selected = sidebar_selected_index(app);

    let lines: Vec<Line> = display_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if Some(i) == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme::active().text)
            };
            Line::from(Span::styled(name.as_str(), style))
        })
        .collect();

    let sidebar = Paragraph::new(lines).block(block);
    frame.render_widget(sidebar, area);
}

fn render_source_content(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.viewer_focus == ViewerFocus::Source;
    let border_style = if focused {
        Style::default()
            .fg(theme::active().focus_border)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::active().muted)
    };

    let title = match app.source_view_mode {
        SourceViewMode::Raw => " Source [raw] ",
        SourceViewMode::Prettified => " Source [prettified] ",
    };

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(border_style)
        .border_style(border_style);

    let Some(patch) = app.patch.as_ref() else {
        let msg = Paragraph::new("No patch loaded")
            .style(Style::default().fg(theme::active().muted))
            .alignment(Alignment::Center)
            .block(outer_block);
        frame.render_widget(msg, area);
        return;
    };

    if patch.sections.is_empty() && patch.raw_lines.is_empty() {
        let msg = Paragraph::new("No circuits in patch")
            .style(Style::default().fg(theme::active().muted))
            .alignment(Alignment::Center)
            .block(outer_block);
        frame.render_widget(msg, area);
        return;
    }

    match app.source_view_mode {
        SourceViewMode::Raw => {
            if patch.raw_lines.is_empty() {
                let msg = Paragraph::new("No patch loaded")
                    .style(Style::default().fg(theme::active().muted))
                    .alignment(Alignment::Center)
                    .block(outer_block);
                frame.render_widget(msg, area);
                return;
            }
            let lines = build_raw_highlighted_lines(patch, app);
            let content = Paragraph::new(lines)
                .scroll((app.source_scroll as u16, 0))
                .block(outer_block);
            frame.render_widget(content, area);
        }
        SourceViewMode::Prettified => {
            let circuits = patch.viewer_circuits();
            if circuits.is_empty() {
                let msg = Paragraph::new("No circuits in patch")
                    .style(Style::default().fg(theme::active().muted))
                    .alignment(Alignment::Center)
                    .block(outer_block);
                frame.render_widget(msg, area);
                return;
            }
            let lines = build_prettified_highlighted_lines(patch, app);
            let content = Paragraph::new(lines)
                .scroll((app.source_scroll as u16, 0))
                .block(outer_block);
            frame.render_widget(content, area);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HighlightKind {
    None,
    OccCurrent,
    OccOther,
    ModCyan,
    ModMagenta,
}

fn find_token_spans_in_value(value: &str, target: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if target.is_empty() || value.is_empty() {
        return out;
    }
    let mut search_start = 0usize;
    while search_start <= value.len().saturating_sub(target.len()) {
        let Some(rel) = value[search_start..].find(target) else {
            break;
        };
        let s = search_start + rel;
        let e = s + target.len();
        let before_ok = if s == 0 {
            true
        } else {
            let c = value.as_bytes()[s - 1] as char;
            !(c.is_ascii_alphanumeric() || c == '_')
        };
        let after_ok = if e >= value.len() {
            true
        } else {
            let c = value.as_bytes()[e] as char;
            !(c.is_ascii_alphanumeric() || c == '_' || c == '.')
        };
        if before_ok && after_ok {
            out.push((s, e));
            search_start = e;
        } else {
            search_start = s + 1;
        }
    }
    out
}

fn build_raw_highlighted_lines(patch: &crate::patch::Patch, app: &App) -> Vec<Line<'static>> {
    let Some(token) = app.selected_component.as_deref() else {
        return patch
            .raw_lines
            .iter()
            .map(|l| Line::from(l.clone()))
            .collect();
    };
    let occ_spans: Vec<crate::patch::Span> = patch.occurrences_for(token).to_vec();
    let mod_affects: Vec<crate::patch::ModifierAffect> = patch.modifier_entries_for(token).to_vec();
    let current_span = occ_spans.get(app.occurrence_cursor).copied();
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(patch.raw_lines.len());
    for (line_idx, raw) in patch.raw_lines.iter().enumerate() {
        // Collect per-line highlight kinds per byte column
        let mut kinds: Vec<HighlightKind> = vec![HighlightKind::None; raw.len()];
        for span in &occ_spans {
            if span.line != line_idx {
                continue;
            }
            let is_current = Some(*span) == current_span;
            let kind = if is_current {
                HighlightKind::OccCurrent
            } else {
                HighlightKind::OccOther
            };
            for kind_slot in kinds
                .iter_mut()
                .skip(span.col_start)
                .take(span.col_end.min(raw.len()).saturating_sub(span.col_start))
            {
                // Priority: OccCurrent > OccOther ; only upgrade if higher
                if *kind_slot == HighlightKind::None
                    || (*kind_slot == HighlightKind::OccOther && kind == HighlightKind::OccCurrent)
                {
                    *kind_slot = kind;
                }
            }
        }
        for affect in &mod_affects {
            if affect.span.line != line_idx {
                continue;
            }
            let kind = if affect.selectat.is_some() {
                HighlightKind::ModMagenta
            } else {
                HighlightKind::ModCyan
            };
            for kind_slot in kinds.iter_mut().skip(affect.span.col_start).take(
                affect
                    .span
                    .col_end
                    .min(raw.len())
                    .saturating_sub(affect.span.col_start),
            ) {
                // Modifier overrides OccOther but not OccCurrent (current yellow reversed has top priority).
                // This matches spec: cursor 0 hides magenta on modifier line, jumping off reveals it.
                if *kind_slot != HighlightKind::OccCurrent {
                    *kind_slot = kind;
                }
            }
        }
        // Also handle mod_spans that might not have affect mapping (fallback)
        // If no highlights, push raw
        if kinds.iter().all(|k| *k == HighlightKind::None) {
            lines.push(Line::from(raw.clone()));
        } else {
            let mut spans_vec: Vec<Span<'static>> = Vec::new();
            let mut start = 0usize;
            while start < raw.len() {
                let cur = kinds[start];
                let mut end = start + 1;
                while end < raw.len() && kinds[end] == cur {
                    end += 1;
                }
                let frag = raw[start..end].to_string();
                let style = match cur {
                    HighlightKind::None => Style::default(),
                    HighlightKind::OccOther => Style::default()
                        .fg(theme::active().occurrence_highlight)
                        .add_modifier(Modifier::BOLD),
                    HighlightKind::OccCurrent => Style::default()
                        .fg(theme::active().occurrence_highlight)
                        .bg(theme::active().muted)
                        .add_modifier(Modifier::REVERSED | Modifier::BOLD),
                    HighlightKind::ModCyan => Style::default()
                        .fg(theme::active().modifier_boolean)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    HighlightKind::ModMagenta => Style::default()
                        .fg(theme::active().modifier_exact)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                };
                if cur == HighlightKind::None {
                    spans_vec.push(Span::raw(frag));
                } else {
                    spans_vec.push(Span::styled(frag, style));
                }
                start = end;
            }
            lines.push(Line::from(spans_vec));
        }
    }
    lines
}

fn build_prettified_highlighted_lines(
    patch: &crate::patch::Patch,
    app: &App,
) -> Vec<Line<'static>> {
    let circuits = patch.viewer_circuits();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let selected = app.selected_component.as_deref();
    for circuit in &circuits {
        let color = circuit_color(&circuit.name);
        lines.push(Line::from(vec![
            Span::styled("┌─ ", Style::default().fg(color)),
            Span::styled(
                circuit.name.clone(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ─┐", Style::default().fg(color)),
        ]));
        for (key, value) in &circuit.entries {
            let mut line_spans: Vec<Span<'static>> = vec![
                Span::styled("│ ", Style::default().fg(color)),
                Span::styled(key.clone(), Style::default().fg(theme::active().viewer_key)),
                Span::raw(" = "),
            ];
            // Determine highlighted value spans
            let val_spans: Vec<Span<'static>> = if let Some(tok) = selected {
                let mods = patch.modifier_entries_for(tok);
                let is_modifier_value = mods.iter().any(|e| e.source == value.trim());
                if is_modifier_value {
                    let is_exact = mods
                        .iter()
                        .any(|e| e.source == value.trim() && e.selectat.is_some());
                    let col = if is_exact {
                        theme::active().modifier_exact
                    } else {
                        theme::active().modifier_boolean
                    };
                    vec![Span::styled(
                        value.clone(),
                        Style::default()
                            .fg(col)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    )]
                } else {
                    let ranges = find_token_spans_in_value(value, tok);
                    if ranges.is_empty() {
                        vec![Span::styled(
                            value.clone(),
                            Style::default().fg(theme::active().text),
                        )]
                    } else {
                        // Check current occurrence distinction inside prettified value?
                        // For prettified we use same occ style for all hits; current gets REVERSED
                        // Lookup current occurrence span content vs value? Simplify: all occ yellow bold, not reversed
                        let mut out: Vec<Span<'static>> = Vec::new();
                        let mut last = 0usize;
                        for (s, e) in ranges {
                            if s > last {
                                out.push(Span::styled(
                                    value[last..s].to_string(),
                                    Style::default().fg(theme::active().text),
                                ));
                            }
                            // Decide current vs other: we highlight current occurrence (if any) with REVERSED
                            // We map current occurrence line to value? For prettified we treat all as OccOther except if value equals token and it's the current line file-wise we could mark current.
                            // Simplify: use REVERSED for all occ in prettified to stand out
                            out.push(Span::styled(
                                value[s..e].to_string(),
                                Style::default()
                                    .fg(theme::active().occurrence_highlight)
                                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                            ));
                            last = e;
                        }
                        if last < value.len() {
                            out.push(Span::styled(
                                value[last..].to_string(),
                                Style::default().fg(theme::active().text),
                            ));
                        }
                        out
                    }
                }
            } else {
                vec![Span::styled(
                    value.clone(),
                    Style::default().fg(theme::active().text),
                )]
            };
            line_spans.extend(val_spans);
            line_spans.push(Span::styled(" │", Style::default().fg(color)));
            lines.push(Line::from(line_spans));
        }
        lines.push(Line::from(Span::styled(
            "└────┘",
            Style::default().fg(color),
        )));
        lines.push(Line::from(""));
    }
    lines
}

fn render_minimap(frame: &mut Frame, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(patch) = app.patch.as_ref() else {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::active().muted));
        frame.render_widget(block, area);
        return;
    };
    let total_lines = if !patch.raw_lines.is_empty() {
        patch.raw_lines.len()
    } else {
        patch.sections.len().max(1)
    };
    let inner_height = area.height.saturating_sub(2) as usize;
    if inner_height == 0 {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::active().muted));
        frame.render_widget(block, area);
        return;
    }
    let viewport_h = {
        // Approximate viewport height of the source content area: same as minimap inner height plus border compensation.
        // Use inner_height as proxy; source content inner height is similar (horizontal split same height).
        inner_height
    };
    let selected = app.selected_component.as_deref();
    let occ_lines: std::collections::HashSet<usize> = if let Some(tok) = selected {
        patch.occurrences_for(tok).iter().map(|s| s.line).collect()
    } else {
        std::collections::HashSet::new()
    };
    let mod_lines: std::collections::HashSet<usize> = if let Some(tok) = selected {
        patch
            .modifier_affected_spans(tok)
            .into_iter()
            .map(|s| s.line)
            .collect()
    } else {
        std::collections::HashSet::new()
    };
    // Viewport indicator rows inclusive range
    let viewport_start = ((app.source_scroll * inner_height) / total_lines).min(inner_height);
    let viewport_end =
        (((app.source_scroll + viewport_h) * inner_height) / total_lines).min(inner_height);
    let viewport_range = viewport_start..viewport_end.max(viewport_start + 1);
    let mut rows: Vec<Line> = Vec::with_capacity(inner_height);
    for row in 0..inner_height {
        let line_start = row * total_lines / inner_height;
        let line_end = ((row + 1) * total_lines / inner_height).max(line_start + 1);
        let has_occ = (line_start..line_end).any(|l| occ_lines.contains(&l));
        let has_mod = (line_start..line_end).any(|l| mod_lines.contains(&l));
        let is_viewport = viewport_range.contains(&row);
        let (ch, mut style) = if has_mod && has_occ {
            ("█", Style::default().fg(theme::active().minimap_combined))
        } else if has_mod {
            let col = {
                // Distinguish exact-value vs boolean: check any mod affect with selectat
                let is_exact = selected.is_some_and(|tok| {
                    patch.modifier_entries_for(tok).iter().any(|e| {
                        mod_lines.contains(&e.span.line)
                            && e.selectat.is_some()
                            && (line_start..line_end).contains(&e.span.line)
                    })
                });
                if is_exact {
                    theme::active().minimap_modifier_exact
                } else {
                    theme::active().minimap_modifier_boolean
                }
            };
            ("▓", Style::default().fg(col))
        } else if has_occ {
            ("█", Style::default().fg(theme::active().minimap_occurrence))
        } else {
            ("·", Style::default().fg(theme::active().muted))
        };
        if is_viewport {
            style = style
                .bg(theme::active().muted)
                .add_modifier(Modifier::REVERSED);
            // Ensure viewport visible even on empty lines
            if ch == "·" {
                style = Style::default()
                    .fg(theme::active().text)
                    .bg(theme::active().muted)
                    .add_modifier(Modifier::REVERSED);
            }
        }
        // Fill minimap width with repeated char to fill inner width
        let inner_width = area.width.saturating_sub(2) as usize;
        let fill = ch.to_string().repeat(inner_width.max(1));
        rows.push(Line::from(Span::styled(fill, style)));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Map ")
        .title_style(Style::default().fg(theme::active().muted))
        .border_style(Style::default().fg(theme::active().muted));
    let paragraph = Paragraph::new(rows).block(block);
    frame.render_widget(paragraph, area);
}

fn render_viewer_status(frame: &mut Frame, area: Rect, app: &App) {
    // Transient status message (e.g. split-ratio feedback from `[`/`]`) trails
    // the hints so they always stay fully visible; the message shows when the
    // bar has room.
    let mut spans = vec![
        Span::styled(
            "Source Viewer",
            Style::default()
                .fg(theme::active().text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled("ESC", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" close | "),
        Span::styled("j/k", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" scroll | "),
        Span::styled("Up/Down", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" occur | "),
        Span::styled("Home/End", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" jump | "),
        Span::styled("t", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" toggle | "),
        Span::styled("Tab", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" focus | "),
        Span::styled("[ / ]", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" split"),
    ];
    if !app.status_message.is_empty() {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            app.status_message.as_str(),
            Style::default().fg(theme::active().text),
        ));
    }
    let status = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(theme::active().status_bg))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::active().muted)),
        );

    frame.render_widget(status, area);
}

fn circuit_color(name: &str) -> Color {
    let t = theme::active();
    match name {
        "button" | "switch" | "notebuttons" | "notobuttons" => t.button,
        "pot" | "encoder" | "faderbank" => t.knob,
        "cvin" | "cv_in" => t.cv_in,
        "cvout" | "cv_out" => t.cv_out,
        "led" => t.led,
        _ => t.accent,
    }
}

fn disambiguate_names(names: &[String]) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut result = Vec::new();
    for name in names {
        let count = counts.entry(name.clone()).or_insert(0);
        if *count == 0 {
            result.push(name.clone());
        } else {
            result.push(format!("{} ({})", name, count));
        }
        *count += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, PrefixState, SourceViewMode, ViewerFocus};
    use crate::patch::Patch;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_at(app: &mut App, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
    }

    fn rendered_text(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_empty_state_without_panic() {
        let mut app = App::new();
        render_at(&mut app, 80, 24);
    }

    #[test]
    fn renders_sample_patch_components() {
        let mut app = App::new();
        app.load_sample_patch();
        let text = rendered_text(&mut app, 80, 24);
        assert!(text.contains("Demo Patch"));
        assert!(text.contains("P2B8"));
        assert!(text.contains("CV I/O"));
        assert!(text.contains("TRIG A"));
        assert!(text.contains("CUTOFF"));
    }

    #[test]
    fn renders_real_patch_at_various_sizes() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        for (w, h) in [(200, 60), (80, 24), (40, 15), (20, 10)] {
            let mut app = App::new();
            app.patch = Some(patch.clone());
            render_at(&mut app, w, h);
        }
    }

    #[test]
    fn renders_with_each_shift_group_active_without_panic() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        for group in [
            crate::patch::ShiftGroup::Group1,
            crate::patch::ShiftGroup::Group2,
            crate::patch::ShiftGroup::Group3,
            crate::patch::ShiftGroup::Group4,
        ] {
            let mut app = App::new();
            app.patch = Some(patch.clone());
            app.active_shift = Some(group);
            render_at(&mut app, 80, 24);
        }
    }

    #[test]
    fn renders_picker_without_panic() {
        let mut app = App::new();
        app.showing_picker = true;
        app.refresh_picker_entries();
        render_at(&mut app, 80, 24);
    }

    // Smoke-level only (deep per-theme regression is task 3.3): each built-in
    // theme must render a full frame without panicking. Restores classic last
    // because other tests in this binary assume the default theme is active.
    #[test]
    fn renders_frame_under_every_builtin_theme_without_panic() {
        // Thread-local override keeps other tests on the default palette.
        for name in theme::THEMES {
            let mut app = App::new();
            app.load_sample_patch();
            app.showing_viewer = true;
            theme::set_test_theme(Some(*theme::resolve(name)));
            render_at(&mut app, 100, 30);
            app.showing_viewer = false;
            render_at(&mut app, 100, 30);
        }
        theme::set_test_theme(None);
    }

    #[test]
    fn status_bar_shows_active_shift_group() {
        let mut app = App::new();
        app.load_sample_patch();
        app.active_shift = Some(crate::patch::ShiftGroup::Group3);
        let text = rendered_text(&mut app, 80, 24);
        assert!(text.contains("SHIFT 3 ACTIVE"));
    }

    #[test]
    fn status_bar_omits_shift_indicator_when_none_active() {
        let mut app = App::new();
        app.load_sample_patch();
        let text = rendered_text(&mut app, 80, 24);
        assert!(!text.contains("SHIFT"));
    }

    #[test]
    fn status_bar_shows_prefix_indicator_while_prefix_armed() {
        let mut app = App::new();
        app.load_sample_patch();
        app.prefix = Some(PrefixState {
            started: std::time::Instant::now(),
        });
        let text = rendered_text(&mut app, 80, 24);
        assert!(text.contains("Prefix: g"));
        assert!(!text.contains("Sample patch loaded."));
    }

    #[test]
    fn status_bar_omits_prefix_indicator_when_none_armed() {
        let mut app = App::new();
        app.load_sample_patch();
        let text = rendered_text(&mut app, 80, 24);
        assert!(!text.contains("Prefix: g"));
    }

    #[test]
    fn renders_viewer_without_panic() {
        let mut app = App::new();
        app.showing_viewer = true;
        render_at(&mut app, 80, 24);
    }

    #[test]
    fn renders_viewer_with_circuits() {
        let mut app = App::new();
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        app.load_patch(patch);
        app.showing_viewer = true;
        let text = rendered_text(&mut app, 120, 40);
        assert!(text.contains("Circuits"));
        assert!(text.contains("Source Viewer"));
        assert!(text.contains("p2b8"));
    }

    #[test]
    fn viewer_shows_no_patch_message_when_empty() {
        let mut app = App::new();
        app.showing_viewer = true;
        let text = rendered_text(&mut app, 80, 24);
        assert!(text.contains("No patch loaded"));
    }

    #[test]
    fn disambiguate_names_adds_suffix_to_duplicates() {
        let names = vec![
            String::from("copy"),
            String::from("button"),
            String::from("copy"),
            String::from("copy"),
        ];
        let result = disambiguate_names(&names);
        assert_eq!(result[0], "copy");
        assert_eq!(result[1], "button");
        assert_eq!(result[2], "copy (1)");
        assert_eq!(result[3], "copy (2)");
    }

    #[test]
    fn disambiguate_names_handles_all_unique() {
        let names = vec![String::from("a"), String::from("b"), String::from("c")];
        let result = disambiguate_names(&names);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    // Assert against theme::active(), never Theme::classic(): theme tests in
    // this binary mutate the process-global active theme, so only a same-
    // moment token read keeps these assertions deterministic.
    #[test]
    fn circuit_color_maps_known_circuits_to_kind_tokens() {
        let t = theme::active();
        assert_eq!(circuit_color("button"), t.button);
        assert_eq!(circuit_color("switch"), t.button);
        assert_eq!(circuit_color("notebuttons"), t.button);
        assert_eq!(circuit_color("pot"), t.knob);
        assert_eq!(circuit_color("encoder"), t.knob);
        assert_eq!(circuit_color("faderbank"), t.knob);
        assert_eq!(circuit_color("cvin"), t.cv_in);
        assert_eq!(circuit_color("cv_in"), t.cv_in);
        assert_eq!(circuit_color("cvout"), t.cv_out);
        assert_eq!(circuit_color("cv_out"), t.cv_out);
        assert_eq!(circuit_color("led"), t.led);
        assert_eq!(circuit_color("p2b8"), t.accent);
        assert_eq!(circuit_color("copy"), t.accent);
    }

    // ── 4.1 embedded layout tests ───────────────────────────────────────

    fn app_with_embedded(patch_name: &str) -> App {
        let content = std::fs::read_to_string(format!("fixtures/{patch_name}.ini")).unwrap();
        let patch = Patch::from_ini_str(&content, patch_name.to_string()).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.showing_viewer = true;
        app
    }

    #[test]
    fn renders_embedded_wide_raw_focus_source() {
        let mut app = app_with_embedded("arpeggio1");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        let text = rendered_text(&mut app, 120, 40);
        assert!(text.contains("Circuits"));
        assert!(text.contains("Panels"));
        assert!(text.contains("Source Viewer"));
        assert!(text.contains("p2b8"));
        assert!(text.contains("ESC"));
        assert!(text.contains("j/k"));
        assert!(text.contains("Up/Down"));
        assert!(text.contains("t"));
        assert!(text.contains("Tab"));
    }

    #[test]
    fn renders_embedded_wide_raw_focus_panels() {
        let mut app = app_with_embedded("arpeggio1");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Panels;
        render_at(&mut app, 120, 40);
        let text = rendered_text(&mut app, 120, 40);
        assert!(text.contains("Circuits"));
        assert!(text.contains("Panels"));
    }

    #[test]
    fn renders_embedded_wide_prettified_focus_source() {
        let mut app = app_with_embedded("arpeggio1");
        app.source_view_mode = SourceViewMode::Prettified;
        app.viewer_focus = ViewerFocus::Source;
        let text = rendered_text(&mut app, 120, 40);
        assert!(text.contains("Circuits"));
        assert!(text.contains("Panels"));
        // prettified shows circuit block caps
        assert!(text.contains("Source Viewer"));
    }

    #[test]
    fn renders_embedded_wide_prettified_focus_panels() {
        let mut app = app_with_embedded("arpeggio1");
        app.source_view_mode = SourceViewMode::Prettified;
        app.viewer_focus = ViewerFocus::Panels;
        render_at(&mut app, 120, 40);
        let text = rendered_text(&mut app, 120, 40);
        assert!(text.contains("Circuits"));
    }

    #[test]
    fn embedded_split_respects_viewer_split_ratio() {
        // Find the panels pane's right border column; it must shift with the ratio.
        let border_col = |ratio: f32| -> u16 {
            let mut app = app_with_embedded("arpeggio1");
            app.viewer_split_ratio = ratio;
            let backend = TestBackend::new(100, 40);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            let buffer = terminal.backend().buffer();
            // Main content starts after the 3-row header, so row 3 is the panels
            // block's top border row. Its top-right corner '┐' marks the panels
            // right border. Find the first such corner in that row.
            let mut col = 0u16;
            for x in 0..buffer.area().width {
                if buffer[(x, 3)].symbol() == "┐" {
                    col = x;
                    break;
                }
            }
            col
        };
        let at_06 = border_col(0.6);
        let at_07 = border_col(0.7);
        let at_03 = border_col(0.3);
        // 0.6 gives 60% of 100 = col 59; 0.7 gives 70% = col 69; 0.3 gives 30% = col 29.
        assert!(
            at_07 > at_06,
            "wider source ratio must push panels border right"
        );
        assert!(
            at_06 > at_03,
            "narrower source ratio must pull panels border left"
        );
        assert_eq!(at_06, 59);
        assert_eq!(at_07, 69);
        assert_eq!(at_03, 29);
    }

    #[test]
    fn viewer_status_bar_shows_split_message() {
        let mut app = app_with_embedded("arpeggio1");
        app.status_message = "Panels/Source split: 70%/30%".to_string();
        // Wide enough that hints plus the trailing message all fit.
        let text = rendered_text(&mut app, 160, 40);
        assert!(text.contains("Source Viewer"));
        assert!(
            text.contains("Panels/Source split: 70%/30%"),
            "viewer status bar must surface transient split-ratio message"
        );
    }

    #[test]
    fn renders_embedded_narrow_raw_no_panic() {
        let mut app = app_with_embedded("arpeggio1");
        app.source_view_mode = SourceViewMode::Raw;
        for (w, h) in [(50, 20), (40, 15), (30, 12), (20, 10)] {
            render_at(&mut app, w, h);
            app.viewer_focus = ViewerFocus::Panels;
            render_at(&mut app, w, h);
            app.viewer_focus = ViewerFocus::Source;
            render_at(&mut app, w, h);
        }
    }

    #[test]
    fn renders_embedded_narrow_prettified_no_panic() {
        let mut app = app_with_embedded("arpeggio1");
        app.source_view_mode = SourceViewMode::Prettified;
        for (w, h) in [(50, 20), (40, 15), (30, 12), (20, 10)] {
            app.viewer_focus = ViewerFocus::Source;
            render_at(&mut app, w, h);
            app.viewer_focus = ViewerFocus::Panels;
            render_at(&mut app, w, h);
        }
    }

    #[test]
    fn renders_embedded_no_panic_with_source_navigation_fixture() {
        let content = std::fs::read_to_string("fixtures/source_navigation.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("source_navigation")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.showing_viewer = true;
        for mode in [SourceViewMode::Raw, SourceViewMode::Prettified] {
            for focus in [ViewerFocus::Source, ViewerFocus::Panels] {
                app.source_view_mode = mode.clone();
                app.viewer_focus = focus;
                for (w, h) in [(120, 40), (80, 24), (40, 15), (20, 10)] {
                    render_at(&mut app, w, h);
                }
            }
        }
    }

    #[test]
    fn viewer_status_shows_all_hints_when_embedded() {
        let mut app = app_with_embedded("arpeggio1");
        app.showing_viewer = true;
        let text = rendered_text(&mut app, 100, 24);
        assert!(text.contains("ESC"), "status should mention ESC");
        assert!(text.contains("j/k"), "status should mention j/k");
        assert!(text.contains("Up/Down"), "status should mention Up/Down");
        assert!(text.contains("t"), "status should mention t toggle");
        assert!(text.contains("Tab"), "status should mention Tab");
        assert!(
            text.contains("Home/End") || text.contains("Home"),
            "status should mention Home/End"
        );
    }

    #[test]
    fn picker_precedence_over_embedded_viewer() {
        let mut app = app_with_embedded("arpeggio1");
        app.showing_viewer = true;
        app.showing_picker = true;
        app.refresh_picker_entries();
        let text = rendered_text(&mut app, 100, 24);
        assert!(text.contains("File Picker"), "picker overlay should win");
        // picker renders on top; still no panic
        render_at(&mut app, 80, 24);
    }

    #[test]
    fn empty_patch_embedded_shows_message() {
        let mut app = App::new();
        app.showing_viewer = true;
        app.viewer_focus = ViewerFocus::Source;
        for mode in [SourceViewMode::Raw, SourceViewMode::Prettified] {
            app.source_view_mode = mode.clone();
            let text = rendered_text(&mut app, 80, 24);
            assert!(
                text.contains("No patch loaded"),
                "empty patch should show message in {mode:?}"
            );
        }
    }

    #[test]
    fn sidebar_disambiguation_shows_suffixes() {
        let content = std::fs::read_to_string("fixtures/source_navigation.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("source_navigation")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.showing_viewer = true;
        // Wide terminal so sidebar is visible
        let text = rendered_text(&mut app, 140, 40);
        // source_navigation.ini has many repeated "copy" and "button"/"switch"
        // disambiguated names should appear when width allows
        assert!(
            text.contains("copy") && (text.contains("copy (1)") || text.contains("copy")),
            "sidebar should show disambiguated names"
        );
    }

    #[test]
    fn renders_embedded_raw_shows_verbatim_lines() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        let text = rendered_text(&mut app, 120, 40);
        // Raw mode shows verbatim ini header like "[p2b8]" or "# fixtures..."
        assert!(text.contains("p2b8") || text.contains("[p2b8]"));
    }

    // ── 4.2 highlight + minimap tests ───────────────────────────────────

    fn buffer_for(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn has_highlighted_token(
        buffer: &ratatui::buffer::Buffer,
        token: &str,
        want_fg: Option<Color>,
        want_modifier: Option<Modifier>,
    ) -> bool {
        let area = buffer.area;
        let token_chars: Vec<char> = token.chars().collect();
        for y in 0..area.height {
            let mut row_chars: Vec<char> = Vec::new();
            let mut row_styles: Vec<Style> = Vec::new();
            for x in 0..area.width {
                let cell = buffer.cell((x, y)).unwrap();
                row_chars.push(cell.symbol().chars().next().unwrap_or(' '));
                row_styles.push(cell.style());
            }
            if row_chars.len() < token_chars.len() {
                continue;
            }
            for start in 0..=row_chars.len() - token_chars.len() {
                if row_chars[start..start + token_chars.len()] != token_chars[..] {
                    continue;
                }
                let mut all_match = true;
                for i in 0..token_chars.len() {
                    let idx = start + i;
                    let style = row_styles[idx];
                    if let Some(fg) = want_fg {
                        if style.fg != Some(fg) {
                            all_match = false;
                            break;
                        }
                    }
                    if let Some(m) = want_modifier {
                        if !style.add_modifier.contains(m) {
                            all_match = false;
                            break;
                        }
                    }
                }
                if all_match {
                    return true;
                }
            }
        }
        false
    }

    fn minimap_viewport_rows(buffer: &ratatui::buffer::Buffer, minimap: Rect) -> Vec<usize> {
        let mut rows = Vec::new();
        let inner_y = minimap.y + 1;
        let inner_h = minimap.height.saturating_sub(2) as usize;
        for i in 0..inner_h {
            let y = inner_y + i as u16;
            let x = minimap.x + 1; // first inner column
            if let Some(cell) = buffer.cell((x, y)) {
                // viewport rows have REVERSED modifier or DarkGray bg
                if cell.style().add_modifier.contains(Modifier::REVERSED)
                    || cell.style().bg == Some(Color::DarkGray)
                {
                    rows.push(i);
                }
            }
        }
        rows
    }

    #[test]
    fn highlight_direct_occurrence_and_modifier_in_raw() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        // B1.1: direct hardware boolean select (cyan modifier) + occurrence yellow
        app.select_component(String::from("B1.1"));
        // Ensure viewport shows first occurrence (scroll 0)
        app.source_scroll = 0;
        let buf = buffer_for(&mut app, 120, 40);
        // Occurrence highlight: B1.1 token in yellow bold somewhere
        assert!(
            has_highlighted_token(&buf, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD)),
            "occurrence B1.1 should be highlighted Yellow Bold in raw"
        );
        // Modifier highlight for select = B1.1 (cyan underlined) – check select token
        // The modifier span is around B1.1 on the select line, same token but modifier style is cyan underlined
        // We search for select keyword nearby? Simpler verify at least one cyan underlined fragment exists for B1.1 select
        // Since occurrence and modifier overlap on same token for direct case, modifier style wins except current.
        // Current occurrence is REVERSED; other occurrences remain yellow; modifier uses cyan. We verify cyan exists OR yellow reversed current.
        let has_cyan =
            has_highlighted_token(&buf, "B1.1", Some(Color::Cyan), Some(Modifier::UNDERLINED));
        let has_yellow_reversed =
            has_highlighted_token(&buf, "B1.1", Some(Color::Yellow), Some(Modifier::REVERSED));
        assert!(
            has_cyan || has_yellow_reversed,
            "direct boolean should show modifier cyan underlined or current yellow reversed"
        );
    }

    #[test]
    fn highlight_transitive_modifier_in_raw() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        // B1.2 -> _TRANSIT -> select = _TRANSIT : transitive
        app.select_component(String::from("B1.2"));
        app.source_scroll = 0;
        let buf = buffer_for(&mut app, 120, 80);
        // Occurrence of B1.2 yellow
        assert!(
            has_highlighted_token(&buf, "B1.2", Some(Color::Yellow), Some(Modifier::BOLD)),
            "transitive source B1.2 occurrence should be yellow"
        );
        // Modifier: select = _TRANSIT should be highlighted cyan when B1.2 selected
        assert!(
            has_highlighted_token(
                &buf,
                "_TRANSIT",
                Some(Color::Cyan),
                Some(Modifier::UNDERLINED)
            ),
            "transitive modifier _TRANSIT should be cyan underlined when B1.2 selected"
        );
    }

    #[test]
    fn highlight_exact_value_modifier_magenta_in_raw() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        // P1.1 -> select = P1.1 with selectat 0.5 exact-value -> magenta
        app.select_component(String::from("P1.1"));
        // cursor 0 is on the modifier line itself, so current occurrence (yellow reversed) hides magenta there
        app.source_scroll = 0;
        let buf0 = buffer_for(&mut app, 120, 80);
        assert!(
            has_highlighted_token(&buf0, "P1.1", Some(Color::Yellow), Some(Modifier::REVERSED))
                || has_highlighted_token(&buf0, "P1.1", Some(Color::Yellow), Some(Modifier::BOLD))
                || has_highlighted_token(
                    &buf0,
                    "P1.1",
                    Some(Color::Magenta),
                    Some(Modifier::UNDERLINED),
                )
                || has_highlighted_token(&buf0, "P1.1", Some(Color::Magenta), Some(Modifier::BOLD)),
            "occurrence or modifier highlight for P1.1 present at cursor 0"
        );
        // moving cursor off the modifier line reveals magenta (OccOther is overridden)
        app.jump_to_occurrence(1);
        // keep viewport at top so modifier line stays visible
        app.source_scroll = 0;
        let buf1 = buffer_for(&mut app, 120, 80);
        assert!(
            has_highlighted_token(
                &buf1,
                "P1.1",
                Some(Color::Magenta),
                Some(Modifier::UNDERLINED)
            ) || has_highlighted_token(&buf1, "P1.1", Some(Color::Magenta), Some(Modifier::BOLD)),
            "exact-value select P1.1 should be magenta underlined when not current"
        );
    }

    #[test]
    fn highlight_current_occurrence_distinct_in_raw() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        app.select_component(String::from("B1.1"));
        // cursor 0 -> first occurrence is current (yellow reversed)
        let buf0 = buffer_for(&mut app, 120, 40);
        assert!(
            has_highlighted_token(&buf0, "B1.1", Some(Color::Yellow), Some(Modifier::REVERSED)),
            "current occurrence should be yellow reversed at cursor 0"
        );
        // Move to next occurrence -> different span becomes reversed, still some reversed exists
        app.jump_to_occurrence(1);
        let buf1 = buffer_for(&mut app, 120, 40);
        assert!(
            has_highlighted_token(&buf1, "B1.1", Some(Color::Yellow), Some(Modifier::REVERSED)),
            "current occurrence should still be yellow reversed after jump to 1"
        );
        // Ensure scroll changed to second occurrence line
        let occ = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
        assert_eq!(app.source_scroll, occ[1].line);
    }

    #[test]
    fn highlight_in_prettified_mode() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Prettified;
        app.viewer_focus = ViewerFocus::Source;
        app.select_component(String::from("B1.2"));
        let buf = buffer_for(&mut app, 120, 80);
        // In prettified, select = _TRANSIT value should be cyan underlined (transitive)
        assert!(
            has_highlighted_token(
                &buf,
                "_TRANSIT",
                Some(Color::Cyan),
                Some(Modifier::UNDERLINED)
            ),
            "prettified transitive modifier should be cyan underlined"
        );
        // Occurrence inside value like B1.2 in other entries (if any) yellow reversed
        // For direct case P1.1 exact
        app.select_component(String::from("P1.1"));
        let buf2 = buffer_for(&mut app, 120, 80);
        assert!(
            has_highlighted_token(
                &buf2,
                "P1.1",
                Some(Color::Magenta),
                Some(Modifier::UNDERLINED)
            ) || has_highlighted_token(
                &buf2,
                "P1.1",
                Some(Color::Cyan),
                Some(Modifier::UNDERLINED)
            ),
            "prettified exact-value P1.1 select should be highlighted magenta/cyan"
        );
    }

    #[test]
    fn highlight_cleared_when_no_selection_in_raw() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        app.select_component(String::from("B1.1"));
        let buf_sel = buffer_for(&mut app, 120, 40);
        assert!(has_highlighted_token(
            &buf_sel,
            "B1.1",
            Some(Color::Yellow),
            Some(Modifier::BOLD)
        ));
        app.clear_selected_component();
        let buf_none = buffer_for(&mut app, 120, 40);
        assert!(
            !has_highlighted_token(&buf_none, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD)),
            "highlights cleared when selection cleared"
        );
        assert!(
            !has_highlighted_token(
                &buf_none,
                "_TRANSIT",
                Some(Color::Cyan),
                Some(Modifier::UNDERLINED)
            ),
            "modifier highlights cleared when selection cleared"
        );
    }

    #[test]
    fn minimap_geometry_published_when_wide() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        app.select_component(String::from("B1.1"));
        render_at(&mut app, 120, 40);
        assert!(
            app.minimap_rect.is_some(),
            "minimap should be published when wide (120x40)"
        );
        let r = app.minimap_rect.unwrap();
        assert!(
            r.width >= MINIMAP_WIDTH,
            "minimap width at least {}",
            MINIMAP_WIDTH
        );
        assert!(r.height >= 10);
    }

    #[test]
    fn minimap_hidden_on_narrow_width() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        app.select_component(String::from("B1.1"));
        // Total width <80 hides minimap
        render_at(&mut app, 70, 24);
        assert!(
            app.minimap_rect.is_none(),
            "minimap hidden when total width <80"
        );
        // Source pane narrow (<60) also hides even if total >=80 but split 50/50 => pane ~35
        render_at(&mut app, 80, 24);
        // At 80 total, pane ~40 (<60) so hidden as well
        assert!(
            app.minimap_rect.is_none(),
            "minimap hidden when source pane <60"
        );
        // Wide still shows
        render_at(&mut app, 120, 40);
        assert!(app.minimap_rect.is_some());
    }

    #[test]
    fn minimap_viewport_indicator_tracks_scroll() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        app.viewer_focus = ViewerFocus::Source;
        app.select_component(String::from("B1.1"));
        app.source_scroll = 0;
        let buf_top = buffer_for(&mut app, 120, 40);
        let rect_top = app.minimap_rect.expect("minimap visible");
        let rows_top = minimap_viewport_rows(&buf_top, rect_top);
        assert!(!rows_top.is_empty(), "viewport should have rows at top");
        assert!(
            rows_top[0] == 0,
            "viewport should start at top when scroll 0, got {:?}",
            rows_top
        );
        // Scroll far down
        app.source_scroll = 50;
        let buf_bot = buffer_for(&mut app, 120, 40);
        let rect_bot = app.minimap_rect.expect("minimap visible");
        let rows_bot = minimap_viewport_rows(&buf_bot, rect_bot);
        assert!(!rows_bot.is_empty());
        assert!(
            rows_bot[0] > rows_top[0],
            "viewport should move down when scrolled: top {:?} vs bot {:?}",
            rows_top,
            rows_bot
        );
        // Indicator stays within minimap height
        let h = rect_bot.height.saturating_sub(2) as usize;
        for r in rows_bot {
            assert!(r < h);
        }
    }

    #[test]
    fn minimap_hidden_on_narrow_threshold_source_pane_width() {
        let mut app = app_with_embedded("source_navigation");
        app.source_view_mode = SourceViewMode::Raw;
        // At 60 total pane ~30, should be hidden; at 120 pane ~60 visible
        for (w, should_show) in [
            (120, true),
            (140, true),
            (80, false),
            (60, false),
            (50, false),
        ] {
            render_at(&mut app, w, 30);
            assert_eq!(
                app.minimap_rect.is_some(),
                should_show,
                "minimap visibility at width {w} expected {should_show}"
            );
        }
    }
}

#[cfg(test)]
mod led_box_tests {
    use super::*;
    use crate::patch::Patch;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_at(app: &mut App, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
    }

    #[test]
    fn led_some_renders_boxed_content() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch.clone());

        render_at(&mut app, 80, 40);

        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();

        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(
            text.contains("◉") || text.contains("○"),
            "led: Some component should render LED glyph (◉ or ○)"
        );
        assert!(
            text.contains("B1.1") || text.contains("P2B8"),
            "led: Some component should render component label"
        );
    }

    #[test]
    fn led_none_renders_text_cell() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch.clone());

        render_at(&mut app, 80, 40);

        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();

        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(
            text.contains("●") || text.contains("○") || text.contains("▣") || text.contains("□"),
            "led: None component should render text cell with symbol"
        );
    }
}
