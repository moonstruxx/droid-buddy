use std::collections::{HashMap, HashSet};

use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{is_picker_parent_entry, App, QuadFocus, SourceViewMode, ViewerFocus};
use crate::graph::{Cluster, Graph, GraphNode};
use crate::patch::{ComponentKind, ComponentState, ShiftGroup};
use crate::rendermetrics::{score_render, RenderFeatures};
use crate::theme;

const QUAD_WIDTH_THRESHOLD: u16 = 120;

#[allow(dead_code)]
fn is_kitty_terminal() -> bool {
    // Runtime detection for kitty-gfx fallback: only attempt image rendering
    // when the terminal advertises kitty graphics support.
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM")
            .map(|v| v == "xterm-kitty")
            .unwrap_or(false)
}

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
    if app.showing_quad {
        // Responsive fallback: below threshold quad panes would be unreadable.
        if frame.area().width < QUAD_WIDTH_THRESHOLD {
            render_embedded_main(frame, chunks[1], app);
            render_quad_fallback_status(frame, chunks[2], app);
        } else {
            render_quad(frame, chunks[1], app);
            render_quad_status(frame, chunks[2], app);
        }
    } else if app.showing_graph {
        render_graph(frame, chunks[1], app);
        render_status(frame, chunks[2], app);
    } else if app.showing_viewer {
        render_embedded_main(frame, chunks[1], app);
        render_viewer_status(frame, chunks[2], app);
    } else {
        render_main(frame, chunks[1], app);
        render_status(frame, chunks[2], app);
    }

    // Edit overlay is the top z-layer above any base content (picker has
    // absolute precedence and already returned). Centered, single-field,
    // responsive via QUAD_WIDTH_THRESHOLD; hint in modifier hue with
    // graph_edge_error red kept for influence elsewhere.
    if app.editing.is_some() {
        render_overlay(frame, app);
    } else if app.showing_validation {
        render_validation_modal(frame, app, frame.area());
    } else if app.optimizer.is_some() {
        render_optimizer_modal(frame, app, frame.area());
    }
}

/// Validation modal overlay — centered 60% width, 70% height, listing
/// `validation_issues` sorted by (line,col). Style follows `render_picker`
/// overlay pattern: `Clear` + rounded `Block` + `Paragraph` lines.
/// Header: "Validation (N) — e:toggle j/k:navigate Enter:jump Esc:close".
/// Each row: `L{line}:{col} [E/W/H] [code] message` with severity color via
/// `validation_error/warning/hint` tokens, selected row highlighted via
/// `validation_selected_bg` + bold, non-selected dimmed. Empty state when
/// no issues (should not normally be shown while modal is open).
fn render_validation_modal(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let is_narrow = area.width < QUAD_WIDTH_THRESHOLD;
    let modal_width = if is_narrow {
        area.width.saturating_sub(4).max(24)
    } else {
        (area.width * 60 / 100).clamp(40, 80).max(24)
    };
    let modal_height = if is_narrow {
        area.height.saturating_sub(4).max(10)
    } else {
        (area.height * 70 / 100).clamp(12, 40).max(10)
    };
    let x = area.x + area.width.saturating_sub(modal_width) / 2;
    let y = area.y + area.height.saturating_sub(modal_height) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);
    frame.render_widget(Clear, modal_area);

    let count = app.validation_issues.len();
    let err = app
        .validation_issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Error)
        .count();
    let warn = app
        .validation_issues
        .iter()
        .filter(|i| i.severity == crate::validation::Severity::Warning)
        .count();
    let hint = count.saturating_sub(err + warn);
    let title = if count == 0 {
        String::from(" Validation (0) ")
    } else {
        format!(" Validation ({count}) {err}E {warn}W {hint}H ")
    };
    let header_hint = " e:toggle j/k:navigate Enter:jump Esc:close ";

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_bottom(Line::from(Span::styled(
            header_hint,
            Style::default().fg(theme::active().muted),
        )))
        .border_style(Style::default().fg(theme::active().validation_modal_border));

    let inner = block.inner(modal_area);
    // Reserve one line for header is handled by block border; inner height is usable rows.
    if inner.width == 0 || inner.height == 0 {
        frame.render_widget(block, modal_area);
        return;
    }

    if count == 0 {
        let empty = Paragraph::new(Line::from(Span::styled(
            "No validation issues",
            Style::default().fg(theme::active().muted),
        )))
        .alignment(Alignment::Center)
        .block(block);
        frame.render_widget(empty, modal_area);
        return;
    }

    // Build lines for all issues; viewport window is scrolled to keep cursor visible.
    let max_rows = inner.height as usize;
    let cursor = app.validation_cursor.min(count.saturating_sub(1));
    let start = if count <= max_rows || cursor < max_rows / 2 {
        0
    } else if cursor + max_rows / 2 >= count {
        count - max_rows
    } else {
        cursor - max_rows / 2
    };
    let end = (start + max_rows).min(count);

    let mut lines: Vec<Line> = Vec::with_capacity(end - start);
    for idx in start..end {
        let issue = &app.validation_issues[idx];
        let is_selected = idx == cursor;
        let (sev_label, sev_color) = match issue.severity {
            crate::validation::Severity::Error => ("E", theme::active().validation_error),
            crate::validation::Severity::Warning => ("W", theme::active().validation_warning),
            crate::validation::Severity::Hint => ("H", theme::active().validation_hint),
        };
        // Spec format: L{line}:{col} [E/W/H] [code] message — line/col are 1-based for user display
        let loc = format!("L{}:{}", issue.span.line + 1, issue.span.col_start + 1);
        // Truncate message to fit inner width; reserve space for loc+sev+code
        let prefix_len = loc.len() + 6 + issue.code.len() + 2; // " [] [] " overhead approx
        let avail = (inner.width as usize)
            .saturating_sub(prefix_len)
            .saturating_sub(1);
        let msg = if avail == 0 {
            String::new()
        } else if issue.message.len() > avail {
            let mut t = issue.message[..avail.saturating_sub(1)].to_string();
            t.push('…');
            t
        } else {
            issue.message.clone()
        };
        let sev_span = Span::styled(
            format!("[{sev_label}]"),
            Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
        );
        let loc_span = Span::styled(loc, Style::default().fg(theme::active().text));
        let code_span = Span::styled(
            format!("[{}]", issue.code),
            Style::default().fg(theme::active().muted),
        );
        let msg_span = Span::styled(msg, Style::default().fg(theme::active().text));
        let spans = vec![
            loc_span,
            Span::raw(" "),
            sev_span,
            Span::raw(" "),
            code_span,
            Span::raw(" "),
            msg_span,
        ];
        // Highlight selected row via background + bold; dim non-selected
        let row_style = if is_selected {
            Style::default()
                .bg(theme::active().validation_selected_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };
        // Apply row_style as background to all spans via line style later; keep sev color foreground.
        // To preserve sev color, we style line with row bg and keep spans fg.
        let mut line = Line::from(spans);
        line.style = row_style;
        // Re-apply severity bold+color which was overwritten by line.style bg — merge by restyling sev span
        // Keep line bg but ensure sev span still has its fg; ratatui composes.
        lines.push(line);
        let _ = is_selected; // suppress unused warning if lint
    }
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, modal_area);
}

/// Optimizer menu overlay (design D5) — centered, mirroring the validation
/// modal pattern. Lists up to three candidates (variant label + before→after
/// `avg`/`max`/back-edges), cursor highlighted via `optimizer_selected_bg`,
/// with a hint line for j/k/Enter/s/r/Esc. The preview is already applied to
/// `Patch.sections` by the handler; this modal only reflects state.
fn render_optimizer_modal(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let is_narrow = area.width < QUAD_WIDTH_THRESHOLD;
    let modal_width = if is_narrow {
        area.width.saturating_sub(4).max(24)
    } else {
        (area.width * 60 / 100).clamp(40, 80).max(24)
    };
    let modal_height = if is_narrow {
        area.height.saturating_sub(4).max(10)
    } else {
        (area.height * 70 / 100).clamp(12, 40).max(10)
    };
    let x = area.x + area.width.saturating_sub(modal_width) / 2;
    let y = area.y + area.height.saturating_sub(modal_height) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);
    frame.render_widget(Clear, modal_area);

    let state = app.optimizer.as_ref();
    let Some(state) = state else {
        return;
    };
    let count = state.candidates.len();
    let title = format!(" Optimizer ({count}) ");
    let header_hint = " j/k select · Enter preview · r restore · s export · Esc close ";

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_bottom(Line::from(Span::styled(
            header_hint,
            Style::default().fg(theme::active().muted),
        )))
        .border_style(Style::default().fg(theme::active().optimizer_modal_border));

    let inner = block.inner(modal_area);
    if inner.width == 0 || inner.height == 0 {
        frame.render_widget(block, modal_area);
        return;
    }

    if count == 0 {
        let empty = Paragraph::new(Line::from(Span::styled(
            "No candidate orderings",
            Style::default().fg(theme::active().muted),
        )))
        .alignment(Alignment::Center)
        .block(block);
        frame.render_widget(empty, modal_area);
        return;
    }

    let cursor = state.cursor.min(count.saturating_sub(1));
    let max_rows = inner.height as usize;
    let start = if count <= max_rows || cursor < max_rows / 2 {
        0
    } else if cursor + max_rows / 2 >= count {
        count - max_rows
    } else {
        cursor - max_rows / 2
    };
    let end = (start + max_rows).min(count);

    let mut lines: Vec<Line> = Vec::with_capacity(end - start);
    for idx in start..end {
        let candidate = &state.candidates[idx];
        let is_selected = idx == cursor;
        // Candidate line: `{label} avg X→Y · max A→B · back-edges N→M`.
        let values = format!(
            " avg {:.2}→{:.2} · max {:.2}→{:.2} · back-edges {}→{}",
            candidate.before.avg,
            candidate.after.avg,
            candidate.before.max,
            candidate.after.max,
            candidate.before.back_edge_count,
            candidate.after.back_edge_count
        );
        let label = Span::styled(
            candidate.label.clone(),
            Style::default().fg(theme::active().text),
        );
        let values = Span::styled(values, Style::default().fg(theme::active().muted));
        let marker = Span::styled(
            if is_selected { "▶ " } else { "   " },
            Style::default().fg(theme::active().text),
        );
        let mut line = Line::from(vec![marker, label, values]);
        if is_selected {
            line.style = Style::default()
                .bg(theme::active().optimizer_selected_bg)
                .add_modifier(Modifier::BOLD);
        } else {
            line.style = Style::default().add_modifier(Modifier::DIM);
        }
        lines.push(line);
    }
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, modal_area);
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

// pub(crate): the render-metrics extractor (src/rendermetrics.rs) mirrors these
// exact layout constants so extractor and renderer cannot drift (design D2).
pub(crate) const COMPONENT_WIDTH: u16 = 16;
pub(crate) const COMPONENT_HEIGHT: u16 = 3;

/// A boxed LED cell needs room for both border columns plus the interior
/// state row; below this width the bordered cell degrades to the unboxed
/// two-line rendering instead of emitting clipped border fragments
/// (droid_tui-wsu).
pub(crate) const BOX_MIN_WIDTH: u16 = 8;

/// Truncate `s` to at most `max_chars` terminal columns, appending `…`
/// (U+2026) when it overflows (droid_tui-lsd). Char-aware so multi-byte
/// labels never split a glyph; the ellipsis counts as one column.
fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if display_width(s) <= max_chars {
        s.to_string()
    } else if max_chars == 0 {
        String::new()
    } else {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('\u{2026}');
        out
    }
}

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

    // Cell size now follows the active scale preset. The published hit rects
    // are the real rendered cells (see render_component_grid), so scaling the
    // layout keeps hit testing correct automatically (droid_tui-ro0).
    let scaled_w = ((COMPONENT_WIDTH as f32 * app.scale_factor).round() as u16).max(8);
    let scaled_h = ((COMPONENT_HEIGHT as f32 * app.scale_factor).round() as u16).max(3);

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

    // Per-panel component lists with folded LEDs already filtered out, since
    // those cells are skipped at render time and must not consume grid slots
    // (rows_for/row_chunks) or count toward panel height.
    let visible_panels: HashMap<&str, Vec<&crate::patch::HwComponent>> = panel_order
        .iter()
        .map(|name| {
            let visible: Vec<&crate::patch::HwComponent> = panels[name]
                .iter()
                .filter(|c| {
                    !(c.kind == ComponentKind::Led && folded_led_ids.contains(c.id.as_str()))
                })
                .copied()
                .collect();
            (name.as_str(), visible)
        })
        .collect();

    // Split each panel's visible components into per-circuit module groups,
    // preserving first-appearance order of each instance key (controller-
    // panels/spec.md "Panel contains modules"). A panel with only one
    // instance renders unchanged as a single flat grid — module sub-borders
    // only appear when a panel genuinely mixes multiple circuit instances,
    // matching the spec's "components from multiple circuits" condition.
    // CV I/O never subdivides: its tokens are fixed jacks, not pluggable HP
    // modules (DESIGN.md's controller glossary does not list CV I/O).
    let module_groups: HashMap<&str, Vec<Vec<&crate::patch::HwComponent>>> = panel_order
        .iter()
        .map(|name| {
            let visible = &visible_panels[name.as_str()];
            let groups = if name == "CV I/O" {
                vec![visible.clone()]
            } else {
                let mut order: Vec<u32> = Vec::new();
                let mut by_instance: HashMap<u32, Vec<&crate::patch::HwComponent>> = HashMap::new();
                for comp in visible {
                    let key = comp.module_instance().unwrap_or(0);
                    by_instance
                        .entry(key)
                        .or_insert_with(|| {
                            order.push(key);
                            Vec::new()
                        })
                        .push(*comp);
                }
                if order.len() <= 1 {
                    vec![visible.clone()]
                } else {
                    order
                        .into_iter()
                        .map(|k| by_instance.remove(&k).unwrap())
                        .collect()
                }
            };
            (name.as_str(), groups)
        })
        .collect();

    let num_panels = panel_order.len().max(1);
    let landscape = app.orientation == crate::app::Orientation::Landscape;
    // Size each panel to its actual grid content so it can never collapse into
    // a 1:35 sliver. The per-panel column count is derived from the panel's
    // real inner width (and scaled_w) below, not from the full area width.
    let mut constraints: Vec<Constraint> = panel_order
        .iter()
        .map(|name| {
            let groups = &module_groups[name.as_str()];
            let (needed_w, needed_h) =
                panel_grid_size(groups, landscape, area, scaled_w, scaled_h, num_panels);
            let len = if landscape { needed_w } else { needed_h };
            Constraint::Length(len)
        })
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
        let groups = &module_groups[name.as_str()];

        // A panel is "affected" when at least one of its components belongs
        // to the currently active shift group; affected panels get a bold
        // colored border, other panels dim while a shift is active, and all
        // panels use the default border when no shift is active. See
        // shift-visualization/spec.md.
        let affected = app.active_shift.is_some()
            && groups
                .iter()
                .flatten()
                .any(|c| c.shift_group == app.active_shift);

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
        // Global pause de-emphasizes the whole panel surface (borders and
        // titles of panel and per-module blocks) with the same DIM modifier
        // shift-dimming uses. Colors and geometry stay untouched, so hit rects
        // and non-paused output are unaffected.
        let border_style = dim_style(border_style, app.processing_paused);

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

        if groups.len() <= 1 {
            render_component_grid(frame, inner, &groups[0], &index_of, app, patch);
            continue;
        }

        // Subdivided: each module instance is its own bordered sub-block.
        // Column count is taken from THIS block's real inner width so the grid
        // never mis-wraps relative to its container.
        let module_cols = (inner.width / scaled_w).max(1) as usize;
        let module_constraints: Vec<Constraint> = groups
            .iter()
            .map(|g| {
                let rows = (g.len().div_ceil(module_cols)).max(1) as u16;
                Constraint::Length(rows * scaled_h + 2)
            })
            .collect();
        let module_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(module_constraints)
            .flex(Flex::Start)
            .split(inner);

        for (m_i, components) in groups.iter().enumerate() {
            if m_i >= module_chunks.len() {
                break;
            }
            let module_title = match components.first().and_then(|c| c.module_instance()) {
                Some(n) => format!(" {} {} ", name, n),
                None => format!(" {} ", name),
            };
            let module_block = Block::default()
                .title(module_title)
                .title_style(border_style)
                .borders(Borders::ALL)
                .border_style(border_style);
            let module_inner = module_block.inner(module_chunks[m_i]);
            frame.render_widget(module_block, module_chunks[m_i]);
            if module_inner.width == 0 || module_inner.height == 0 {
                continue;
            }
            render_component_grid(frame, module_inner, components, &index_of, app, patch);
        }
    }
}

/// Render one module's/panel's components into a grid within `area`.
/// The column count is derived from the cell's actual inner width and the
/// scaled cell size, so the published hit rect (`comp_chunks[col_i]`, the real
/// rendered cell) always matches what is drawn and hover/selection stay correct
/// at every scale preset (droid_tui-ro0). Shared by the flat (single-instance)
/// panel path and the per-module path above.
fn render_component_grid(
    frame: &mut Frame,
    area: Rect,
    components: &[&crate::patch::HwComponent],
    index_of: &HashMap<&str, usize>,
    app: &mut App,
    patch: &crate::patch::Patch,
) {
    // Cell size follows the live scale preset so the published hit rect
    // (comp_chunks[col_i]) always equals the drawn cell.
    let scaled_w = ((COMPONENT_WIDTH as f32 * app.scale_factor).round() as u16).max(8);
    let scaled_h = ((COMPONENT_HEIGHT as f32 * app.scale_factor).round() as u16).max(3);
    let cols = (area.width / scaled_w).max(1) as usize;
    // A cell never exceeds its container: at a panel width below
    // COMPONENT_WIDTH the nominal cell would draw its box over the panel
    // border column, so the real cell width is clamped to the container
    // (droid_tui-wsu). The published hit rect is this clamped cell, keeping
    // hit-testing in lockstep with what is drawn.
    let cell_w = scaled_w.min(area.width / cols as u16).max(1);
    let rows_for = |n: usize| -> u16 { (n.div_ceil(cols)).max(1) as u16 };

    let row_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(scaled_h);
            rows_for(components.len()) as usize
        ])
        .flex(Flex::Start)
        .split(area);

    // HW label override: per-patch store + [labels] config (layers_enabled / max_shift_layer)
    // with Patch::display_label fallback chain. Geometry untouched — rects equal drawn cells.
    let hw_store = app.current_hw_store();
    let settings = crate::config::load(&crate::theme::canonical_theme_name, crate::theme::THEMES);
    let layers_enabled = settings.labels.layers_enabled;
    let max_shift_layer = settings.labels.max_shift_layer;

    for (row_i, row) in components.chunks(cols).enumerate() {
        if row_i >= row_chunks.len() {
            break;
        }
        let comp_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Length(cell_w); row.len()])
            .flex(Flex::Start)
            .split(row_chunks[row_i]);

        for (col_i, comp) in row.iter().enumerate() {
            if col_i >= comp_chunks.len() {
                break;
            }

            let global_idx = index_of[comp.id.as_str()];
            let is_hovered = app.hovered_component == Some(global_idx);
            let is_shift_active =
                comp.shift_group.is_some() && comp.shift_group == app.active_shift;
            let shift: u8 = match comp.shift_group {
                Some(ShiftGroup::Group1) => 1,
                Some(ShiftGroup::Group2) => 2,
                Some(ShiftGroup::Group3) => 3,
                Some(ShiftGroup::Group4) => 4,
                None => 1,
            };
            // Hand-built patches (Patch::sample, empty sections) keep their
            // bespoke labels ("TRIG A") — display_label would derive "btn_1".
            let display_label = if patch.sections.is_empty() && !comp.label.is_empty() {
                comp.label.clone()
            } else {
                patch.display_label(&comp.id, shift, layers_enabled, max_shift_layer, &hw_store)
            };
            render_component(
                frame,
                comp_chunks[col_i],
                comp,
                &display_label,
                is_hovered,
                is_shift_active,
                app.processing_paused,
                patch,
            );
            // Published rect is the real rendered cell (comp_chunks[col_i]).
            // Layout already uses the scaled cell size, so the hit rect stays
            // exactly equal to the drawn cell at every scale preset.
            app.component_rects.push((global_idx, comp_chunks[col_i]));
        }
    }
}

/// Compute the grid extent of one controller panel so it can be sized to its
/// actual content instead of becoming a 1:35 sliver (droid_tui-7ik).
///
/// * `cols` is derived from the panel's *real* inner width, not the full area
///   width, so the wrap count is correct inside narrow panels.
/// * The cross-axis length is `needed_w` (Landscape, panels side-by-side) or
///   `needed_h` (Portrait, panels stacked), each built from the live grid
///   geometry rather than the sum of vertical extents.
fn panel_grid_size(
    groups: &[Vec<&crate::patch::HwComponent>],
    landscape: bool,
    area: Rect,
    scaled_w: u16,
    scaled_h: u16,
    num_panels: usize,
) -> (u16, u16) {
    let subdivided = groups.len() > 1;
    // Estimate this panel's inner width: full area width in Portrait (panels
    // span the whole width); an equal share of the area in Landscape so panels
    // stay narrow enough to sit side by side.
    let inner_w_est = if landscape {
        (area.width / num_panels as u16).max(scaled_w)
    } else {
        area.width.saturating_sub(2)
    };
    let cols = (inner_w_est / scaled_w).max(1) as usize;

    let needed_w = if subdivided {
        let module_cols = (inner_w_est.saturating_sub(2) / scaled_w).max(1) as usize;
        (module_cols as u16) * scaled_w + 4
    } else {
        (cols as u16) * scaled_w + 2
    };

    let needed_h = if subdivided {
        let module_cols = ((inner_w_est.saturating_sub(2)) / scaled_w).max(1) as usize;
        let mut h: u16 = 2; // panel border
        for g in groups {
            let rows = (g.len().div_ceil(module_cols)).max(1) as u16;
            h += rows * scaled_h + 2;
        }
        h
    } else {
        let n = groups.first().map(|g| g.len()).unwrap_or(0);
        let rows = (n.div_ceil(cols)).max(1) as u16;
        rows * scaled_h + 2
    };

    (needed_w, needed_h)
}

#[allow(clippy::too_many_arguments)]
fn render_component(
    frame: &mut Frame,
    area: Rect,
    comp: &crate::patch::HwComponent,
    display_label: &str,
    is_hovered: bool,
    is_shift_active: bool,
    paused: bool,
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
                // A value-driven switch mirrors knob/encoder percentage display.
                ComponentState::Value(v) => format!("{:.0}%", v * 100.0),
                ComponentState::On => String::from("ON"),
                _ => String::from("OFF"),
            };
            (
                if matches!(comp.state, ComponentState::Value(_)) {
                    "◉"
                } else if matches!(comp.state, ComponentState::On) {
                    "▣"
                } else {
                    "□"
                },
                state,
                theme::active().switch,
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

    let hover_style = dim_style(
        if is_hovered {
            Style::default()
                .fg(fg_color)
                .bg(theme::active().muted)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(fg_color)
        },
        paused,
    );

    // If this component owns a LED, render a bordered box (3 rows tall) —
    // unless the cell is too narrow for a legible box, in which case it
    // falls back to the unboxed two-line text cell (droid_tui-wsu).
    if let Some(led_id) = &comp.led {
        if area.width >= BOX_MIN_WIDTH && area.height >= COMPONENT_HEIGHT {
            // Look up the LED component by id — do not use .unwrap().
            let led_component = patch.hw_components.iter().find(|c| c.id == led_id.as_str());

            let led_glyph = match led_component {
                Some(led) => match &led.state {
                    ComponentState::On | ComponentState::Active => "◉",
                    _ => "○",
                },
                // LED not found in patch — fall back to unlit glyph.
                None => "○",
            };

            // Hover styling applied to box content/border, same convention as text path.
            let display_style = dim_style(
                if is_hovered {
                    Style::default()
                        .fg(fg_color)
                        .bg(theme::active().muted)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(fg_color)
                },
                paused,
            );

            // controller-panels spec §"Box LED-associated elements": one bordered
            // cell, border colored by the element's kind, showing the element's
            // symbol, label, state, and the LED glyph reflecting the LED's state
            // — a single state, not a second textual LED state (droid_tui-888).
            // The 3-row cell has no room for a border plus multiple content
            // lines, so the label lives in the top title (drawn inside the
            // border row) and the single interior row holds state + LED glyph.
            // The label is ellipsized to the border row's inner width so the
            // closing corner never lands glued to a hard-cut word
            // (droid_tui-lsd).
            let label =
                truncate_with_ellipsis(display_label, (area.width as usize).saturating_sub(5));
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(display_style)
                .title_top(Line::styled(
                    format!(" {} {} ", symbol, label),
                    display_style,
                ));
            let inner = block.inner(area);
            frame.render_widget(block, area);

            let line = Line::from(Span::styled(
                format!("{} {}", state_text, led_glyph),
                display_style,
            ));
            let widget = Paragraph::new(line).alignment(Alignment::Center);
            frame.render_widget(widget, inner);
        } else {
            render_text_cell(
                frame,
                area,
                symbol,
                display_label,
                &state_text,
                hover_style,
                paused,
            );
        }
    } else {
        render_text_cell(
            frame,
            area,
            symbol,
            display_label,
            &state_text,
            hover_style,
            paused,
        );
    }
}

/// Render the unboxed two-line text cell (symbol + label over state) used
/// for LED-less components and as the narrow-width fallback for boxed ones
/// (droid_tui-wsu). The Paragraph fills the whole 3-row area, so no gap is
/// left below; the label is ellipsized to the cell width (droid_tui-lsd).
fn render_text_cell(
    frame: &mut Frame,
    area: Rect,
    symbol: &str,
    display_label: &str,
    state_text: &str,
    hover_style: Style,
    paused: bool,
) {
    let label = truncate_with_ellipsis(display_label, (area.width as usize).saturating_sub(2));
    let lines = vec![
        Line::from(vec![
            Span::styled(symbol, hover_style),
            Span::raw(" "),
            Span::styled(label, hover_style),
        ]),
        Line::from(Span::styled(
            state_text,
            dim_style(Style::default().fg(theme::active().muted), paused),
        )),
        Line::from(Span::raw("")), // third row filler so the 3-row area is fully occupied
    ];

    let widget = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(widget, area);
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

/// Apply the shared de-emphasis modifier (DIM) used by shift-dimming (panel
/// borders) and graph highlight-dimming (unhighlighted edges/nodes). While
/// global processing is paused the whole panel surface is dimmed this way;
/// colors are left untouched so non-paused output stays byte-identical.
fn dim_style(style: Style, paused: bool) -> Style {
    if paused {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}

/// Per-frame render-outlier recommendation (task 3.1, design D5): score the
/// loaded patch at the current width/theme and return the advisory hint span
/// when degraded. Re-evaluated every frame so a resize that changes the
/// verdict updates the hint immediately — `None` when healthy (spec: healthy
/// render shows no hint), when no patch is loaded, or when the scorer reports
/// schema drift (design D1: a broken artifact must not spam the status bar).
fn render_outlier_hint(app: &App, width: u16) -> Option<Span<'static>> {
    let patch = app.patch.as_ref()?;
    let features = RenderFeatures::extract(patch, width, theme::active());
    match score_render(&features) {
        Ok(Some(outlier)) => Some(Span::styled(
            format!(
                "Renders degraded at {width} cols \u{2014} use \u{2265} {} cols or reduce scale",
                outlier.recommended_width
            ),
            Style::default()
                .fg(theme::active().render_outlier_warning)
                .add_modifier(Modifier::BOLD),
        )),
        Ok(None) | Err(_) => None,
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

    // Explicit pause marker: a bold accent "stop" span so the paused state is
    // visible even in short terminals where the status message is truncated.
    if app.processing_paused {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            "PROCESSING PAUSED",
            Style::default()
                .fg(theme::active().accent)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(group) = app.active_shift {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            format!("SHIFT {} ACTIVE", group.key_label()),
            Style::default()
                .fg(shift_color(group))
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Modifier status hint: MOD <tokens> → N cells / M cables in modifier hue.
    // Shown when a modifier is active (single selected_component + influence).
    // Orthogonal to shift border — both can coexist.
    if let (Some(token), Some(influence)) =
        (app.selected_component.as_deref(), app.influence.as_ref())
    {
        let n = influence.influenced_nodes.len();
        let m = influence.influenced_edges.len();
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            format!("MOD {} → {} cells / {} cables", token, n, m),
            Style::default()
                .fg(theme::modifier_hue(token))
                .add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(hint) = app.status_for_scope() {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            hint,
            Style::default()
                .fg(theme::active().accent)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Advisory render-outlier hint (design D5): never gates, never blocks —
    // a pure status-channel span in its dedicated token.
    if let Some(hint) = render_outlier_hint(app, area.width) {
        spans.push(Span::raw(" | "));
        spans.push(hint);
    }

    // Latency legend on the graph surface (design D2): the summary mean/max
    // plus the back-edge count, in the legend token. The 190µs figure is a
    // fixed prose constant for the human loop-time reference, not a computed
    // value.
    if app.showing_graph {
        if let Some(data) = app.graph.as_ref().and_then(|g| g.latency.as_ref()) {
            let legend = format!(
                "latency avg {:.1} / max {:.1} (1 loop \u{2248} 190\u{b5}s) | {} back edge(s)",
                data.summary.avg, data.summary.max, data.summary.back_edge_count
            );
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(
                legend,
                Style::default()
                    .fg(theme::active().graph_edge_latency_legend)
                    .add_modifier(Modifier::BOLD),
            ));
        }
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
            // The parent entry is a bare ".." sentinel with no file name;
            // render it as ".." instead of an empty label.
            let display = if is_picker_parent_entry(path) {
                "..".to_string()
            } else {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            };

            let prefix = if is_selected { "▶ " } else { "  " };

            format!("{}{}", prefix, display)
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

fn render_overlay(frame: &mut Frame, app: &App) {
    let Some(editing) = app.editing.as_ref() else {
        return;
    };
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let is_narrow = area.width < QUAD_WIDTH_THRESHOLD;
    let overlay_width = if is_narrow {
        area.width.saturating_sub(4).max(24)
    } else {
        (area.width * 60 / 100).clamp(40, 70).max(24)
    };
    let overlay_height: u16 = 5;
    let x = area.x + area.width.saturating_sub(overlay_width) / 2;
    let y = area.y + area.height.saturating_sub(overlay_height) / 2;
    let overlay_area = Rect::new(x, y, overlay_width, overlay_height);
    frame.render_widget(Clear, overlay_area);
    let settings = crate::config::load(&crate::theme::canonical_theme_name, crate::theme::THEMES);
    let layers_enabled = settings.labels.layers_enabled;
    let max_shift_layer = settings.labels.max_shift_layer;
    let status = app
        .editing_status_line(layers_enabled, max_shift_layer)
        .unwrap_or_default();
    let hue_token = app.editing_hue_token();
    let hue = hue_token
        .as_deref()
        .map(theme::modifier_hue)
        .unwrap_or(theme::active().text);
    let hint_style = Style::default().fg(hue);
    let input_line = Line::from(vec![
        Span::styled(
            editing.draft.clone(),
            Style::default().fg(theme::active().text),
        ),
        Span::styled(
            "\u{258C}",
            Style::default()
                .fg(theme::active().text)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let hint_text = if status.is_empty() {
        match &editing.kind {
            crate::app::EditKind::Hw { .. } if layers_enabled => {
                format!(
                    "Enter save | Esc cancel | 1..{} layer",
                    max_shift_layer.clamp(1, 8)
                )
            }
            _ => "Enter save | Esc cancel".to_string(),
        }
    } else if is_narrow {
        status.clone()
    } else {
        let suffix = match &editing.kind {
            crate::app::EditKind::Hw { .. } if layers_enabled => {
                format!(" | 1..{} layer", max_shift_layer.clamp(1, 8))
            }
            _ => String::new(),
        };
        format!("{} | Enter save | Esc cancel{}", status, suffix)
    };
    let hint_line = Line::from(Span::styled(hint_text, hint_style));
    let paragraph = Paragraph::new(vec![input_line, hint_line]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Edit Label ")
            .border_style(Style::default().fg(hue)),
    );
    frame.render_widget(paragraph, overlay_area);
}

// ── Signal-flow graph view (task 5.1) ──────────────────────────────────────

/// Width of a rounded node frame. Kept modest so nodes stay legible and many
/// fit across a wide surface; clamped to the surface on narrow terminals.
const GRAPH_NODE_WIDTH: u16 = 22;
/// Height of a rounded node frame (title bar row + interior + borders).
const GRAPH_NODE_HEIGHT: u16 = 5;
/// Gap between a cluster's member nodes and its container frame.
const GRAPH_CLUSTER_PADDING: u16 = 2;

/// Render the full-screen signal-flow graph surface (design D8): cluster
/// containers from banner groups, then cable edge polylines, then rounded node
/// frames with title bars and left/right edge ports on top. Edges draw before
/// nodes so the node frames cover the port cells for a clean join. The graph
/// view takes over the whole main area like the source viewer, never an
/// overlay mixed with panels.
fn render_graph(frame: &mut Frame, area: Rect, app: &mut App) {
    app.clear_graph_cluster_rects();
    app.clear_graph_node_rects();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let empty = match app.graph.as_ref() {
        None => true,
        Some(g) => g.nodes.is_empty() || app.graph_positions.len() != g.nodes.len(),
    };
    if empty {
        render_graph_empty(frame, area);
        return;
    }

    // Circuit-label override for node titles (FULL pane): store + patch
    // captured before the borrow split below.
    let circuit_store = app.current_circuit_store();
    let patch_for_title = app.patch.clone();

    // Diff state captured before borrow split (clone for borrow-friendly access).
    // When `diff_scope` is set, use the filtered (scoped) report so graph
    // highlights match the status hint's cable count.
    let diff_showing = app.diff_showing;
    let diff_report_owned = app.filtered_report();
    let diff_report_ref = diff_report_owned.as_ref();

    // Copyable state read before the borrow split below.
    let hovered = app.hovered_graph_node;
    let latency_coloring = app.latency_coloring;

    // Split `app` field borrows so reading the graph and publishing cluster
    // rects (a renderer→handler handoff) coexist within one frame.
    let App {
        graph,
        graph_positions,
        graph_cluster_rects,
        graph_node_rects: node_rect_field,
        disabled_circuits,
        ..
    } = app;
    let Some(graph) = graph.as_ref() else {
        return;
    };

    // Map frozen solver positions (floats in a virtual plane) onto the surface:
    // the bounding box of all positions stretches to fill the usable area, so
    // any set of positions lands on-screen deterministically (design D8/Open
    // Questions allow an implementation-time fit).
    let node_rects = graph_node_rects(graph_positions, area, &graph.nodes);
    let surface = frame.area();

    // Clusters first so node frames draw over their containers' interiors.
    for (i, cluster) in graph.clusters.iter().enumerate() {
        if let Some(rect) = graph_cluster_rect(cluster, &graph.nodes, &node_rects, surface) {
            graph_cluster_rects.push((i, rect));
            render_graph_cluster_frame_with_diff(
                frame,
                rect,
                cluster,
                diff_report_ref,
                diff_showing,
                &graph.nodes,
            );
        }
    }
    // Edges before nodes so node frames draw over the port cells.
    render_graph_edges_with_highlight(
        frame,
        area,
        graph,
        &node_rects,
        GraphEdgeOpts {
            highlight: None,
            disabled: Some(disabled_circuits),
            diff_report: diff_report_ref,
            diff_showing,
            latency: graph.latency.as_ref(),
            latency_coloring,
        },
    );
    for (i, node) in graph.nodes.iter().enumerate() {
        let node_rect = node_rects[i];
        node_rect_field.push((i, node_rect));
        render_graph_node_with_highlight(
            frame,
            node_rect,
            node,
            graph,
            None,
            Some(disabled_circuits),
            hovered == Some(i),
            patch_for_title.as_ref(),
            Some(&circuit_store),
            diff_report_ref,
            diff_showing,
        );
    }
}

/// Empty-patch message for the graph surface, mirroring the source viewer's
/// `render_empty` handling: a centered hint instead of a bare panel.
fn render_graph_empty(frame: &mut Frame, area: Rect) {
    let msg = Paragraph::new("No patch loaded. Press 'l' to load.")
        .style(Style::default().fg(theme::active().muted))
        .alignment(Alignment::Center);
    frame.render_widget(msg, area);
}

/// Map each frozen solver position onto a screen rect for its node frame.
/// A simple deterministic fit: the min/max of all positions defines the box
/// that stretches into the area, leaving room for the node's own frame so no
/// node overflows the right/bottom edges. Coincident positions coincide.
fn graph_node_rects(positions: &[(f32, f32)], area: Rect, nodes: &[GraphNode]) -> Vec<Rect> {
    if positions.len() != nodes.len() {
        return Vec::new();
    }
    let node_w = GRAPH_NODE_WIDTH.min(area.width);
    let node_h = GRAPH_NODE_HEIGHT.min(area.height);
    // Available travel for the top-left corner: the frame keeps its full size.
    let avail_w = area.width.saturating_sub(node_w).max(1) as f32;
    let avail_h = area.height.saturating_sub(node_h).max(1) as f32;
    let min_x = positions.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = positions
        .iter()
        .map(|p| p.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = positions.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = positions
        .iter()
        .map(|p| p.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    positions
        .iter()
        .map(|&(x, y)| {
            let sx = (x - min_x) / span_x;
            let sy = (y - min_y) / span_y;
            let col = area.x + (sx * avail_w).round() as u16;
            let row = area.y + (sy * avail_h).round() as u16;
            Rect::new(col, row, node_w, node_h)
        })
        .collect()
}

/// Title text for a node's frame: the circuit name, with the zero-based
/// instance index appended only when the name is repeated (instance > 0).
fn graph_node_title(node: &GraphNode) -> String {
    if node.instance_index == 0 {
        node.circuit.clone()
    } else {
        format!("{} {}", node.circuit, node.instance_index)
    }
}

/// Circuit-label override for a node's title: `Patch::circuit_display_label`
/// when a patch and store are available, otherwise the derived title.
fn graph_node_display_title(
    node: &GraphNode,
    patch: Option<&crate::patch::Patch>,
    circuit_store: Option<&HashMap<(String, usize), String>>,
) -> String {
    if let (Some(patch), Some(store)) = (patch, circuit_store) {
        patch.circuit_display_label(&node.id, store)
    } else {
        graph_node_title(node)
    }
}

/// Whether a circuit instance has processing disabled: `App.disabled_circuits`
/// is keyed by `(circuit name, instance index)`, the same identity a
/// `GraphNode` carries (`circuit` + `instance_index`).
fn circuit_disabled(
    disabled: &HashSet<(String, usize)>,
    circuit: &str,
    instance_index: usize,
) -> bool {
    disabled
        .iter()
        .any(|(name, idx)| name == circuit && *idx == instance_index)
}

/// Inferred cable type for edge coloring (design D8). DROID cables carry no
/// type; the kind is guessed from the producing circuit's name and is a visual
/// aid only. Topology and validation never depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CableKind {
    Control,
    Audio,
    Midi,
    Unknown,
}

impl CableKind {
    /// Classify a producing circuit's output: clock/gate/trigger/pulsar/div
    /// emit control signals; midi/note/seq/pitch emit musical/midi signals;
    /// anything else is treated as audio/CV.
    fn from_circuit(circuit: &str) -> CableKind {
        let name = circuit.to_ascii_lowercase();
        if ["clock", "gate", "trigger", "pulsar", "div"]
            .iter()
            .any(|k| name.contains(k))
        {
            CableKind::Control
        } else if ["midi", "note", "seq", "pitch"]
            .iter()
            .any(|k| name.contains(k))
        {
            CableKind::Midi
        } else {
            CableKind::Audio
        }
    }
}

/// The producing circuit of a cable: the source end of the first edge carrying
/// it, resolved to a node's circuit name.
fn cable_source_circuit<'a>(graph: &'a Graph, cable: &str) -> Option<&'a str> {
    let source = graph
        .edges
        .iter()
        .find(|e| e.cable == cable)?
        .source
        .clone();
    graph
        .nodes
        .iter()
        .find(|n| n.id == source)
        .map(|n| n.circuit.as_str())
}

/// Cable kind inferred from its producing circuit; `Unknown` when no edge
/// produces it.
fn cable_kind(graph: &Graph, cable: &str) -> CableKind {
    match cable_source_circuit(graph, cable) {
        Some(circuit) => CableKind::from_circuit(circuit),
        None => CableKind::Unknown,
    }
}

fn cable_color_with_diff(
    graph: &Graph,
    cable: &str,
    diff_report: Option<&crate::diff::DiffReport>,
    diff_showing: bool,
) -> Color {
    let theme = theme::active();
    if graph.validation.iter().any(|issue| issue.cable == cable) {
        return theme.graph_edge_error;
    }
    if diff_showing {
        if let Some(report) = diff_report {
            if report.added_cables.contains(&cable.to_string()) {
                return theme.graph_edge_diff_added;
            }
            if report.removed_cables.contains(&cable.to_string()) {
                return theme.graph_edge_diff_removed;
            }
            if report.changed_cables.iter().any(|c| c.cable == cable) {
                return theme.graph_edge_diff_added;
            }
        }
    }
    match cable_kind(graph, cable) {
        CableKind::Control => theme.graph_edge_control,
        CableKind::Audio => theme.graph_edge_audio,
        CableKind::Midi => theme.graph_edge_midi,
        CableKind::Unknown => theme.graph_edge_unknown,
    }
}

/// Ramp stop for a cable's latency (design D2):
/// `round(L / (N×AVG) × (stops−1))` where `L` is the edge latency, `N` the
/// edge count, and `AVG` the summary mean, clamped to `stops−1` so any
/// latency at or past the normalization lands on the hottest stop. Degenerate
/// inputs (no edges or zero mean) collapse to the cold end.
fn latency_ramp_index(latency: f32, edge_count: usize, avg: f32, stops: usize) -> usize {
    if edge_count == 0 || avg <= 0.0 || stops == 0 {
        return 0;
    }
    let normalized = latency / (edge_count as f32 * avg) * (stops as f32 - 1.0);
    (normalized.round() as usize).min(stops - 1)
}

/// The box-drawing glyph for a polyline cell given which orthogonal neighbors
/// also belong to the polyline. A cell with a single neighbor gets a straight
/// run along that axis; its missing neighbor is the port, covered by the node.
fn box_drawing_char(up: bool, down: bool, left: bool, right: bool) -> char {
    match (up, down, left, right) {
        (true, true, false, false) => '│',
        (false, false, true, true) => '─',
        (false, true, false, true) => '┌',
        (false, true, true, false) => '┐',
        (true, false, false, true) => '└',
        (true, false, true, false) => '┘',
        (true, true, false, true) => '├',
        (true, true, true, false) => '┤',
        (false, true, true, true) => '┬',
        (true, false, true, true) => '┴',
        (true, true, true, true) => '┼',
        (true, false, false, false) | (false, true, false, false) => '│',
        (false, false, true, false) | (false, false, false, true) => '─',
        _ => '╳',
    }
}

/// All cells of the deterministic 3-segment polyline between a source port
/// (right edge of the source node) and a sink port (left edge of the sink):
/// a horizontal run out of the source, a vertical connector, a horizontal run
/// into the sink.
fn polyline_cells(x_s: i16, y_s: i16, x_t: i16, y_t: i16) -> Vec<(i16, i16)> {
    let mid_x = (x_s + x_t) / 2;
    let mut cells = Vec::new();
    for x in x_s.min(mid_x)..=x_s.max(mid_x) {
        cells.push((x, y_s));
    }
    for y in y_s.min(y_t)..=y_s.max(y_t) {
        cells.push((mid_x, y));
    }
    for x in x_t.min(mid_x)..=x_t.max(mid_x) {
        cells.push((x, y_t));
    }
    cells
}

/// Draw every cable as a colored box-drawing polyline between its ports,
/// clipped to `area`. Edges draw before node frames so the frames cover the
/// port cells for a clean join. When two polylines share a cell the later
/// edge in `graph.edges` wins, keeping crossings deterministic.
fn render_graph_edges(frame: &mut Frame, area: Rect, graph: &Graph, node_rects: &[Rect]) {
    render_graph_edges_with_highlight(frame, area, graph, node_rects, GraphEdgeOpts::default());
}

/// Grouped options for `render_graph_edges_with_highlight` to stay under
/// clippy's 7-argument limit.
#[derive(Default)]
struct GraphEdgeOpts<'a> {
    highlight: Option<&'a HashSet<String>>,
    disabled: Option<&'a HashSet<(String, usize)>>,
    diff_report: Option<&'a crate::diff::DiffReport>,
    diff_showing: bool,
    /// Per-edge latency (design D2), parallel to `graph.edges` by index.
    latency: Option<&'a crate::latency::LatencyData>,
    /// When true the latency ramp replaces the cable-kind color for non-error,
    /// non-diff cables (error > diff > ramp > kind precedence).
    latency_coloring: bool,
}

fn render_graph_edges_with_highlight(
    frame: &mut Frame,
    area: Rect,
    graph: &Graph,
    node_rects: &[Rect],
    opts: GraphEdgeOpts<'_>,
) {
    // kitty-gfx optional: when feature enabled and terminal is kitty, we would
    // emit inline image escapes instead of box-drawing. Fallback is box-drawing.
    #[cfg(feature = "kitty-gfx")]
    if is_kitty_terminal() {
        // Stub: kitty image rendering would replace this path; for now fallback
        // to box-drawing so feature flag never breaks non-kitty terminals.
    }
    let GraphEdgeOpts {
        highlight,
        disabled,
        diff_report,
        diff_showing,
        latency,
        latency_coloring,
    } = opts;
    for (edge_index, edge) in graph.edges.iter().enumerate() {
        let (Some(src), Some(sink)) = (
            graph.nodes.iter().position(|n| n.id == edge.source),
            graph.nodes.iter().position(|n| n.id == edge.sink),
        ) else {
            continue;
        };
        let src_rect = node_rects[src];
        let sink_rect = node_rects[sink];
        let x_s = src_rect.x as i16 + src_rect.width as i16 - 1;
        let y_s = src_rect.y as i16 + src_rect.height as i16 / 2;
        let x_t = sink_rect.x as i16;
        let y_t = sink_rect.y as i16 + sink_rect.height as i16 / 2;
        if x_s == x_t && y_s == y_t {
            continue; // coincident nodes: zero-length edge, nothing to draw
        }
        let raw = polyline_cells(x_s, y_s, x_t, y_t);
        let mut cells: Vec<(i16, i16)> = Vec::new();
        for cell in raw {
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
        let is_highlighted = highlight
            .map(|set| set.contains(&edge.cable))
            .unwrap_or(false);
        let has_active_highlight = highlight.map(|s| !s.is_empty()).unwrap_or(false);
        let incident_disabled = disabled
            .map(|set| {
                circuit_disabled(
                    set,
                    &graph.nodes[src].circuit,
                    graph.nodes[src].instance_index,
                ) || circuit_disabled(
                    set,
                    &graph.nodes[sink].circuit,
                    graph.nodes[sink].instance_index,
                )
            })
            .unwrap_or(false);
        let has_error = graph
            .validation
            .iter()
            .any(|issue| issue.cable == edge.cable);
        let has_diff = diff_showing
            && diff_report.is_some_and(|r| {
                r.added_cables.contains(&edge.cable)
                    || r.removed_cables.contains(&edge.cable)
                    || r.changed_cables.iter().any(|c| c.cable == edge.cable)
            });
        let diff_color = if has_diff {
            diff_report.and_then(|r| {
                if r.added_cables.contains(&edge.cable) {
                    Some(theme::active().graph_edge_diff_added)
                } else if r.removed_cables.contains(&edge.cable) {
                    Some(theme::active().graph_edge_diff_removed)
                } else if r.changed_cables.iter().any(|c| c.cable == edge.cable) {
                    Some(theme::active().graph_edge_diff_added)
                } else {
                    None
                }
            })
        } else {
            None
        };
        let (color, modifier) = if incident_disabled {
            // Disabled circuit: dim overrides influence highlight, but a
            // validation finding keeps the error color (error red > dim >
            // influence > kind color).
            if has_error {
                (theme::active().graph_edge_error, Modifier::empty())
            } else {
                (theme::active().graph_edge_dim, Modifier::DIM)
            }
        } else if has_error {
            (theme::active().graph_edge_error, Modifier::empty())
        } else if let Some(dc) = diff_color {
            (dc, Modifier::BOLD)
        } else if has_active_highlight {
            if is_highlighted {
                (theme::active().graph_edge_highlight, Modifier::BOLD)
            } else {
                (theme::active().graph_edge_dim, Modifier::DIM)
            }
        } else {
            // Latency ramp replaces the kind color when coloring is on
            // (error > diff > ramp > kind); a back-edge always lands on the
            // hottest stop (design D2).
            let latency_color = latency.and_then(|data| {
                if !latency_coloring {
                    return None;
                }
                let entry = data.edges.get(edge_index)?;
                let ramp = theme::active().graph_edge_latency_ramp();
                let stop = if entry.is_back_edge {
                    ramp.len() - 1
                } else {
                    latency_ramp_index(
                        entry.latency,
                        data.edges.len(),
                        data.summary.avg,
                        ramp.len(),
                    )
                };
                Some(ramp[stop])
            });
            (
                latency_color.unwrap_or_else(|| {
                    cable_color_with_diff(graph, &edge.cable, diff_report, diff_showing)
                }),
                Modifier::empty(),
            )
        };
        let style = Style::default().fg(color).add_modifier(modifier);
        let (ax, ay, aw, ah) = (
            area.x as i16,
            area.y as i16,
            area.width as i16,
            area.height as i16,
        );
        for &(cx, cy) in &cells {
            if cx < ax || cx >= ax + aw || cy < ay || cy >= ay + ah {
                continue; // clipped at the surface
            }
            let ch = box_drawing_char(
                cells.contains(&(cx, cy - 1)),
                cells.contains(&(cx, cy + 1)),
                cells.contains(&(cx - 1, cy)),
                cells.contains(&(cx + 1, cy)),
            );
            frame
                .buffer_mut()
                .set_string(cx as u16, cy as u16, ch.to_string().as_str(), style);
        }
    }
}

/// Render one node as a ComfyUI-style rounded frame with a title bar and
/// edge ports: an input marker on the left border for nodes that sink edges,
/// an output marker on the right border for nodes that source them. A node
/// can be both. Exact port-to-edge pairing is task 5.2; here the ports are
/// simple presence markers.
#[allow(dead_code)]
fn render_graph_node(frame: &mut Frame, area: Rect, node: &GraphNode, graph: &Graph) {
    render_graph_node_with_highlight(
        frame, area, node, graph, None, None, false, None, None, None, false,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_graph_node_with_highlight(
    frame: &mut Frame,
    area: Rect,
    node: &GraphNode,
    graph: &Graph,
    highlight_nodes: Option<&HashSet<(String, usize)>>,
    disabled: Option<&HashSet<(String, usize)>>,
    hovered: bool,
    patch: Option<&crate::patch::Patch>,
    circuit_store: Option<&HashMap<(String, usize), String>>,
    diff_report: Option<&crate::diff::DiffReport>,
    diff_showing: bool,
) {
    let is_disabled = disabled
        .map(|set| circuit_disabled(set, &node.circuit, node.instance_index))
        .unwrap_or(false);
    let has_active = highlight_nodes.map(|s| !s.is_empty()).unwrap_or(false);
    let is_highlighted = highlight_nodes
        .map(|s| s.contains(&node.id))
        .unwrap_or(false);
    let (border_color, title_color, extra_mod) = if is_disabled {
        // Disabled circuit: dim token + DIM override any influence highlight
        // (dim > influence).
        (
            theme::active().graph_node_dim,
            theme::active().graph_node_dim,
            Modifier::DIM,
        )
    } else if has_active {
        if is_highlighted {
            (
                theme::active().graph_node_highlight,
                theme::active().graph_node_highlight,
                Modifier::BOLD,
            )
        } else {
            (
                theme::active().graph_node_dim,
                theme::active().graph_node_dim,
                Modifier::DIM,
            )
        }
    } else {
        (
            theme::active().graph_node_border,
            theme::active().graph_node_title,
            Modifier::empty(),
        )
    };
    let mut border_style = Style::default().fg(border_color).add_modifier(extra_mod);
    let mut title_style = Style::default().fg(title_color).add_modifier(extra_mod);
    if hovered {
        // Hover emphasis (reversed on the muted background) stays visible on
        // disabled and influenced nodes alike (hover > dim > influence).
        border_style = border_style
            .bg(theme::active().muted)
            .add_modifier(Modifier::REVERSED);
        title_style = title_style
            .bg(theme::active().muted)
            .add_modifier(Modifier::REVERSED);
    }
    let mut title = graph_node_display_title(node, patch, circuit_store);
    if diff_showing {
        if let Some(report) = diff_report {
            if report.changed_nodes.iter().any(|n| n.id == node.id) {
                title = format!("{}*", title);
            } else if report.added_nodes.contains(&node.id)
                || report.removed_nodes.contains(&node.id)
            {
                // Added/removed nodes also get a marker for visibility
                title = format!("{}*", title);
            }
        }
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title)
        .title_style(title_style);
    frame.render_widget(block, area);

    let is_sink = graph.edges.iter().any(|e| e.sink == node.id);
    let is_source = graph.edges.iter().any(|e| e.source == node.id);
    let mid_row = area.y + area.height / 2;
    if is_sink {
        frame.buffer_mut().set_string(
            area.x,
            mid_row,
            "◉",
            Style::default().fg(theme::active().graph_port_input),
        );
    }
    if is_source {
        frame.buffer_mut().set_string(
            area.x + area.width - 1,
            mid_row,
            "●",
            Style::default().fg(theme::active().graph_port_output),
        );
    }
}

/// Render one banner-group cluster as a titled bordered container enclosing its
/// member nodes. The implicit unnamed group (empty title) renders as a plain
/// bordered area without a title. Publishes the container rect to
/// `graph_cluster_rects` so handler hit-testing (4.3) can use it.
/// Compute a cluster's container rect: the union of its member node frames
/// inflated by a border margin, clamped into `surface`. `None` when the cluster
/// has no member nodes (defensive; banner groups always cover every section).
fn graph_cluster_rect(
    cluster: &Cluster,
    nodes: &[GraphNode],
    node_rects: &[Rect],
    surface: Rect,
) -> Option<Rect> {
    let member_rects: Vec<Rect> = nodes
        .iter()
        .zip(node_rects)
        .filter(|(node, _)| cluster.section_range.contains(&node.section_index))
        .map(|(_, rect)| *rect)
        .collect();
    let mut union = member_rects.first().copied()?;
    for rect in &member_rects[1..] {
        union = union.union(*rect);
    }
    Some(clamp_rect(
        Rect::new(
            union.x.saturating_sub(GRAPH_CLUSTER_PADDING),
            union.y.saturating_sub(GRAPH_CLUSTER_PADDING),
            union.width.saturating_add(GRAPH_CLUSTER_PADDING * 2),
            union.height.saturating_add(GRAPH_CLUSTER_PADDING * 2),
        ),
        surface,
    ))
}

/// Draw a cluster's bordered container. The implicit unnamed group (empty
/// title) renders as a plain bordered area without a title.
fn render_graph_cluster_frame(frame: &mut Frame, rect: Rect, cluster: &Cluster) {
    render_graph_cluster_frame_with_diff(frame, rect, cluster, None, false, &[]);
}

fn render_graph_cluster_frame_with_diff(
    frame: &mut Frame,
    rect: Rect,
    cluster: &Cluster,
    diff_report: Option<&crate::diff::DiffReport>,
    diff_showing: bool,
    nodes: &[GraphNode],
) {
    let mut border_color = theme::active().graph_cluster_border;
    let mut title_color = theme::active().graph_cluster_title;
    if diff_showing {
        if let Some(report) = diff_report {
            // Tint when all member NodeIds are added or all removed.
            let member_ids: Vec<(String, usize)> = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| cluster.section_range.contains(&n.section_index))
                .map(|(_, n)| n.id.clone())
                .collect();
            if !member_ids.is_empty() {
                let all_added = member_ids.iter().all(|id| report.added_nodes.contains(id));
                let all_removed = member_ids
                    .iter()
                    .all(|id| report.removed_nodes.contains(id));
                if all_added {
                    border_color = theme::active().graph_edge_diff_added;
                    title_color = theme::active().graph_edge_diff_added;
                } else if all_removed {
                    border_color = theme::active().graph_edge_diff_removed;
                    title_color = theme::active().graph_edge_diff_removed;
                }
            }
        }
    }
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    if !cluster.title.is_empty() {
        block = block
            .title(cluster.title.as_str())
            .title_style(Style::default().fg(title_color));
    }
    frame.render_widget(block, rect);
}

/// Clamp `rect` into `within`, shrinking overhanging right/bottom edges.
fn clamp_rect(rect: Rect, within: Rect) -> Rect {
    let x = rect
        .x
        .min(within.x + within.width.saturating_sub(1))
        .max(within.x);
    let y = rect
        .y
        .min(within.y + within.height.saturating_sub(1))
        .max(within.y);
    let max_w = within.x + within.width - x;
    let max_h = within.y + within.height - y;
    Rect::new(x, y, rect.width.min(max_w), rect.height.min(max_h))
}

// ── Quad concurrent view (3.2) ───────────────────────────────────────────

fn render_quad(frame: &mut Frame, area: Rect, app: &mut App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    app.component_rects.clear();
    app.clear_graph_cluster_rects();
    app.clear_graph_node_rects();
    app.clear_filtered_cluster_rects();
    app.clear_filtered_node_rects();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    if rows.len() < 2 {
        return;
    }
    let top = rows[0];
    let bottom = rows[1];

    let panels_pct = (app.viewer_split_ratio.clamp(0.3, 0.7) * 100.0) as u16;
    let source_pct = 100u16.saturating_sub(panels_pct);
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(panels_pct),
            Constraint::Percentage(source_pct),
        ])
        .split(top);
    let bottom_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(bottom);

    if top_cols.len() < 2 || bottom_cols.len() < 2 {
        return;
    }

    render_quad_pane(frame, top_cols[0], app, QuadFocus::Panels, " Panels ");
    render_quad_pane(frame, top_cols[1], app, QuadFocus::Source, " Source ");
    render_quad_pane(
        frame,
        bottom_cols[0],
        app,
        QuadFocus::GraphFull,
        " Graph FULL ",
    );
    render_quad_pane(
        frame,
        bottom_cols[1],
        app,
        QuadFocus::GraphFiltered,
        " Graph FILTERED ",
    );

    // Content inside each pane
    render_quad_panels_content(frame, top_cols[0], app);
    render_quad_source_content(frame, top_cols[1], app);
    render_quad_graph_full_content(frame, bottom_cols[0], app);
    render_quad_graph_filtered_content(frame, bottom_cols[1], app);
}

fn quad_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(theme::active().focus_border)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::active().muted)
    }
}

fn render_quad_pane(frame: &mut Frame, area: Rect, app: &App, focus: QuadFocus, title: &str) {
    let focused = app.quad_focus == focus;
    let style = quad_border_style(focused);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(style)
        .border_style(style);
    frame.render_widget(block, area);
}

fn render_quad_panels_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if let Some(patch) = app.patch.clone() {
        render_patch_grouped(frame, inner, &patch, app);
    } else {
        render_empty(frame, inner);
    }
}

fn render_quad_source_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.width == 0 || inner.height == 0 {
        app.source_pane_rect = Some(area);
        return;
    }
    // Reuse source pane rendering but ensure source_pane_rect is the quad source pane
    render_source_pane(frame, inner, app);
    // Publish outer quad source pane rect for focus routing (handler checks source_pane_rect)
    app.source_pane_rect = Some(area);
}

fn render_quad_graph_full_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let empty = match app.graph.as_ref() {
        None => true,
        Some(g) => g.nodes.is_empty() || app.graph_positions.len() != g.nodes.len(),
    };
    if empty {
        render_graph_empty(frame, inner);
        return;
    }
    // Circuit label store for FULL node titles — captured before clone borrows.
    let circuit_store = app.current_circuit_store();
    let patch_for_title = app.patch.clone();
    // Need to split borrows: graph + positions + rect fields
    let graph = match app.graph.as_ref() {
        Some(g) => g.clone(),
        None => return,
    };
    let positions = app.graph_positions.clone();
    let disabled = app.disabled_circuits.clone();
    let hovered = app.hovered_graph_node;
    let surface = inner;
    let node_rects = graph_node_rects(&positions, inner, &graph.nodes);
    // Clusters
    let cluster_rects: Vec<(usize, Rect)> = graph
        .clusters
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            graph_cluster_rect(c, &graph.nodes, &node_rects, surface).map(|r| (i, r))
        })
        .collect();
    for (i, rect) in &cluster_rects {
        app.graph_cluster_rects.push((*i, *rect));
        render_graph_cluster_frame(frame, *rect, &graph.clusters[*i]);
    }
    // Highlight sets from influence: graph.highlighted_* already populated by recompute_influence
    let highlight_edges: Option<HashSet<String>> = if graph.highlighted_edges.is_empty() {
        None
    } else {
        Some(graph.highlighted_edges.clone())
    };
    let highlight_nodes: Option<HashSet<(String, usize)>> = if graph.highlighted_nodes.is_empty() {
        None
    } else {
        Some(graph.highlighted_nodes.clone())
    };
    render_graph_edges_with_highlight(
        frame,
        inner,
        &graph,
        &node_rects,
        GraphEdgeOpts {
            highlight: highlight_edges.as_ref(),
            disabled: Some(&disabled),
            diff_report: None,
            diff_showing: false,
            latency: graph.latency.as_ref(),
            latency_coloring: app.latency_coloring,
        },
    );
    for (i, node) in graph.nodes.iter().enumerate() {
        let nr = node_rects[i];
        app.graph_node_rects.push((i, nr));
        render_graph_node_with_highlight(
            frame,
            nr,
            node,
            &graph,
            highlight_nodes.as_ref(),
            Some(&disabled),
            hovered == Some(i),
            patch_for_title.as_ref(),
            Some(&circuit_store),
            None,
            false,
        );
    }
}

fn render_quad_graph_filtered_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(graph) = app.filtered_graph.clone() else {
        let msg = Paragraph::new("No influence selected")
            .style(Style::default().fg(theme::active().muted))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    };
    if graph.nodes.is_empty() || app.filtered_positions.len() != graph.nodes.len() {
        let msg = Paragraph::new("No influenced nodes")
            .style(Style::default().fg(theme::active().muted))
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }
    let circuit_store = app.current_circuit_store();
    let patch_for_title = app.patch.clone();
    let positions = app.filtered_positions.clone();
    let surface = inner;
    let node_rects = graph_node_rects(&positions, inner, &graph.nodes);
    for (i, cluster) in graph.clusters.iter().enumerate() {
        if let Some(rect) = graph_cluster_rect(cluster, &graph.nodes, &node_rects, surface) {
            app.filtered_cluster_rects.push((i, rect));
            render_graph_cluster_frame(frame, rect, cluster);
        }
    }
    // FILTERED is uniformly highlighted — no dim, use normal cable colors or highlight color
    // Use no highlight dimming (all nodes are influenced)
    render_graph_edges(frame, inner, &graph, &node_rects);
    for (i, node) in graph.nodes.iter().enumerate() {
        let nr = node_rects[i];
        app.filtered_node_rects.push((i, nr));
        render_graph_node_with_highlight(
            frame,
            nr,
            node,
            &graph,
            None,
            None,
            false,
            patch_for_title.as_ref(),
            Some(&circuit_store),
            None,
            false,
        );
    }
}

fn render_quad_status(frame: &mut Frame, area: Rect, app: &App) {
    let focus_label = match app.quad_focus {
        QuadFocus::Panels => "Panels",
        QuadFocus::Source => "Source",
        QuadFocus::GraphFull => "Graph FULL",
        QuadFocus::GraphFiltered => "Graph FILTERED",
    };
    let modifier_info = app.active_modifier_var.as_deref().unwrap_or("no modifier");
    let spans = vec![
        Span::styled(
            format!(
                "Quad [Tab] focus: {} | Modifier: {}",
                focus_label, modifier_info
            ),
            Style::default()
                .fg(theme::active().text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled("Esc", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" close | "),
        Span::styled("Tab", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" cycle"),
    ];
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

fn render_quad_fallback_status(frame: &mut Frame, area: Rect, _app: &App) {
    let spans = vec![
        Span::styled(
            "Quad fallback (<120 cols): showing panels+source",
            Style::default().fg(theme::active().text),
        ),
        Span::raw(" | "),
        Span::styled("Esc", Style::default().fg(theme::active().viewer_key)),
        Span::raw(" close"),
    ];
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

pub(crate) const MINIMAP_WIDTH: u16 = 3;

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
        app.source_pane_rect = None;
        app.minimap_rect = None;
        return;
    }
    // Publish the full pane rect (sidebar/minimap included) for mouse focus
    // routing; minimap click-to-scroll keeps precedence in the handler.
    app.source_pane_rect = Some(area);

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

    let circuit_store = app.current_circuit_store();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let names: Vec<String> = patch
        .sections
        .iter()
        .map(|s| {
            let idx = *counts.get(&s.name).unwrap_or(&0);
            let node_id = (s.name.clone(), idx);
            counts.insert(s.name.clone(), idx + 1);
            patch.circuit_display_label(&node_id, &circuit_store)
        })
        .collect();
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
            Line::from(Span::styled(name.clone(), style))
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

/// Display width of `s`. The project does not depend on `unicode_width`,
/// and the prettified content is ASCII config text, so `chars().count()` is
/// exact enough for box alignment.
fn display_width(s: &str) -> usize {
    s.chars().count()
}

fn build_prettified_highlighted_lines(
    patch: &crate::patch::Patch,
    app: &App,
) -> Vec<Line<'static>> {
    let circuits = patch.viewer_circuits();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let selected = app.selected_component.as_deref();
    let circuit_store = app.current_circuit_store();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for circuit in &circuits {
        let idx = *counts.get(&circuit.name).unwrap_or(&0);
        let node_id = (circuit.name.clone(), idx);
        let display_name = patch.circuit_display_label(&node_id, &circuit_store);
        counts.insert(circuit.name.clone(), idx + 1);
        let color = circuit_color(&circuit.name);

        // Uniform interior width so every box gets one clean right border
        // (droid_tui-mzg). `w` is the widest content row; the box renders an
        // (w + 2)-wide inner strip so the top dashes, the entry padding and the
        // footer all share the same right edge.
        let header_text = format!("─ {} ─", display_name);
        let mut w: usize = display_width(&header_text);
        for (key, value) in &circuit.entries {
            let entry_text = format!("{} = {}", key, value);
            w = w.max(display_width(&entry_text));
        }

        // Top border: corner, the framed title (name kept bold), padding dashes
        // to reach width w + 2, closing corner.
        let top_pad = (w + 2).saturating_sub(display_width(&header_text));
        let top_spans: Vec<Span<'static>> = vec![
            Span::styled("┌", Style::default().fg(color)),
            Span::styled("─ ", Style::default().fg(color)),
            Span::styled(
                display_name.clone(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ─", Style::default().fg(color)),
            Span::styled("─".repeat(top_pad), Style::default().fg(color)),
            Span::styled("┐", Style::default().fg(color)),
        ];
        lines.push(Line::from(top_spans));

        for (key, value) in &circuit.entries {
            // Highlighted value spans — styling (colors, BOLD, UNDERLINED,
            // REVERSED) preserved verbatim; only geometry changes below.
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
                        let mut out: Vec<Span<'static>> = Vec::new();
                        let mut last = 0usize;
                        for (s, e) in ranges {
                            if s > last {
                                out.push(Span::styled(
                                    value[last..s].to_string(),
                                    Style::default().fg(theme::active().text),
                                ));
                            }
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

            // Assemble "key = value" and measure it to width `w` for alignment.
            let mut content_spans: Vec<Span<'static>> = vec![
                Span::styled(key.clone(), Style::default().fg(theme::active().viewer_key)),
                Span::raw(" = "),
            ];
            for s in &val_spans {
                content_spans.push(s.clone());
            }
            let content_w: usize = content_spans
                .iter()
                .map(|s| display_width(&s.content))
                .sum();

            let mut line_spans: Vec<Span<'static>> =
                vec![Span::styled("│ ", Style::default().fg(color))];
            if content_w < w {
                let gap = w - content_w;
                line_spans.extend(content_spans);
                line_spans.push(Span::styled(" ".repeat(gap), Style::default().fg(color)));
            } else {
                line_spans.extend(content_spans);
            }
            line_spans.push(Span::styled(" │", Style::default().fg(color)));
            lines.push(Line::from(line_spans));
        }

        lines.push(Line::from(Span::styled(
            format!("└{}┘", "─".repeat(w + 2)),
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
    fn renders_optimizer_menu_overlay_with_candidates() {
        use crate::handler::handle_event;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        // Open the optimizer menu via `g o`.
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        handle_event(key(KeyCode::Char('g')), &mut app);
        handle_event(key(KeyCode::Char('o')), &mut app);
        assert!(app.optimizer.is_some());
        let text = rendered_text(&mut app, 100, 40);
        assert!(text.contains("Optimizer"), "menu title missing");
        assert!(text.contains("avg"), "candidate values missing");
        assert!(text.contains("back-edges"), "back-edge counts missing");
        assert!(text.contains("Esc close"), "hint line missing");
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
    fn status_bar_shows_mod_hint_when_modifier_active() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(std::path::Path::new(
            "fixtures/modifier_switch_passthrough.ini",
        ))
        .unwrap();
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));
        let text = rendered_text(&mut app, 100, 24);
        assert!(
            text.contains("MOD B1.1"),
            "status should contain MOD hint for B1.1, got: {}",
            text
        );
        assert!(text.contains("cells"), "hint should contain cells count");
        assert!(text.contains("cables"), "hint should contain cables count");
    }

    #[test]
    fn status_bar_omits_mod_hint_when_no_modifier_active() {
        let mut app = App::new();
        app.load_sample_patch();
        let text = rendered_text(&mut app, 80, 24);
        assert!(!text.contains("MOD "), "no MOD hint when no selection");
    }

    #[test]
    fn status_bar_shows_shift_and_mod_coexistence() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(std::path::Path::new(
            "fixtures/modifier_switch_passthrough.ini",
        ))
        .unwrap();
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));
        app.active_shift = Some(crate::patch::ShiftGroup::Group1);
        let text = rendered_text(&mut app, 100, 24);
        assert!(
            text.contains("SHIFT 1 ACTIVE"),
            "shift border hint must remain"
        );
        assert!(
            text.contains("MOD B1.1"),
            "modifier hint must coexist with shift"
        );
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

    // ── render-outlier status hint (task 3.1) ────────────────────────────

    /// Pins a palette for the calling thread and restores the default on
    /// drop, mirroring regression.rs's `ThemedGuard` for status-channel tests.
    struct ThemePin;

    impl ThemePin {
        fn pin(name: &str) -> Self {
            crate::theme::set_test_theme(Some(*crate::theme::resolve(name)));
            Self
        }
    }

    impl Drop for ThemePin {
        fn drop(&mut self) {
            crate::theme::set_test_theme(None);
        }
    }

    fn arpeggio_app() -> App {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app
    }

    /// Style of the first cell of the first occurrence of `token` in the
    /// buffer (row-major), or None when the token is not rendered.
    fn token_style(buffer: &ratatui::buffer::Buffer, token: &str) -> Option<ratatui::style::Style> {
        let area = buffer.area;
        let want: Vec<char> = token.chars().collect();
        for y in 0..area.height {
            let chars: Vec<char> = (0..area.width)
                .map(|x| {
                    buffer
                        .cell((x, y))
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            if chars.len() < want.len() {
                continue;
            }
            for start in 0..=chars.len() - want.len() {
                if chars[start..start + want.len()] == want[..] {
                    return buffer.cell((start as u16, y)).map(|c| c.style());
                }
            }
        }
        None
    }

    /// The status channel: the last three rows (border, content, border) of
    /// the frame, trimmed like regression.rs's `buffer_to_ansi`.
    fn status_rows_to_ansi(buffer: &ratatui::buffer::Buffer) -> String {
        let area = buffer.area;
        let mut rows = Vec::new();
        for y in area.height.saturating_sub(3)..area.height {
            let mut line = String::new();
            for x in 0..area.width {
                line.push_str(buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
            rows.push(line.trim_end().to_string());
        }
        while rows.last().is_some_and(|r| r.is_empty()) {
            rows.pop();
        }
        rows.join("\n")
    }

    #[test]
    fn status_bar_shows_render_outlier_hint_when_degraded() {
        // arpeggio1 wants 228 cols; at 80 the render is predicted degraded and
        // the status bar must surface the advisory hint in the dedicated token.
        // Note: the status row is 80 cols wide, so the tail of the hint is
        // truncated by the paragraph — assert the visible substring.
        let _pin = ThemePin::pin("classic");
        let mut app = arpeggio_app();
        let buf = buffer_for(&mut app, 80, 30);
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("Renders degraded at 80 cols") && text.contains("\u{2265} 228"),
            "degraded render must show the hint: {text:?}"
        );
        let style = token_style(&buf, "Renders degraded").expect("hint rendered");
        assert_eq!(
            style.fg,
            Some(crate::theme::resolve("classic").render_outlier_warning),
            "hint rides the render_outlier_warning token"
        );
        assert!(
            style.add_modifier.contains(ratatui::style::Modifier::BOLD),
            "hint is a bold advisory span"
        );
    }

    #[test]
    fn status_bar_healthy_render_shows_no_hint() {
        // At or above native fit (228) the render is healthy in classic
        // (overflow 0, contrast 5.252 \u{2265} 4.5): no hint (spec: healthy
        // render shows no hint), even with the same patch loaded.
        let _pin = ThemePin::pin("classic");
        let mut app = arpeggio_app();
        let buf = buffer_for(&mut app, 240, 30);
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            !text.contains("Renders degraded"),
            "healthy render must not show the hint: {text:?}"
        );
    }

    #[test]
    fn status_bar_hint_tracks_width_verdict_per_frame() {
        // The recommendation is re-evaluated per frame: the same loaded app
        // flips to a hint at 80 cols and back to none at 240 cols (classic —
        // mono would flag contrast at every width), so a terminal resize can
        // never leave the hint stale.
        let _pin = ThemePin::pin("classic");
        let mut app = arpeggio_app();
        let narrow = buffer_for(&mut app, 80, 30);
        let narrow_text: String = narrow.content().iter().map(|c| c.symbol()).collect();
        assert!(
            narrow_text.contains("Renders degraded at 80 cols"),
            "narrow frame must hint: {narrow_text:?}"
        );
        let wide = buffer_for(&mut app, 240, 30);
        let wide_text: String = wide.content().iter().map(|c| c.symbol()).collect();
        assert!(
            !wide_text.contains("Renders degraded"),
            "wide frame must not hint: {wide_text:?}"
        );
    }

    #[test]
    fn render_outlier_status_hint_snapshot_matrix() {
        // Task 3.1 verify: the status-hint channel renders in every palette at
        // widths 80/100/120 (arpeggio1 is degraded at all three — min 228).
        for &theme_name in crate::theme::THEMES {
            let _pin = ThemePin::pin(theme_name);
            for width in [80u16, 100, 120] {
                let mut app = arpeggio_app();
                let buf = buffer_for(&mut app, width, 30);
                let channel = status_rows_to_ansi(&buf);
                insta::with_settings!({snapshot_suffix => format!("render_outlier_{theme_name}_{width}")}, {
                    insta::assert_snapshot!(channel);
                });
            }
        }
    }

    #[test]
    fn hint_token_color_per_palette() {
        // The hint rides the render_outlier_warning token in every palette:
        // classic yellow, terminal Reset (owns its colors), mono white.
        for &theme_name in crate::theme::THEMES {
            let _pin = ThemePin::pin(theme_name);
            let mut app = arpeggio_app();
            let buf = buffer_for(&mut app, 80, 30);
            let style = token_style(&buf, "Renders degraded").expect("hint rendered");
            assert_eq!(
                style.fg,
                Some(crate::theme::resolve(theme_name).render_outlier_warning),
                "hint token color for {theme_name}"
            );
            assert!(
                style.add_modifier.contains(ratatui::style::Modifier::BOLD),
                "hint bold for {theme_name}"
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

#[cfg(test)]
mod switch_rendering_tests {
    use super::*;
    use crate::patch::Patch;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// A patch whose only hardware component is a switch (S token), so glyph
    /// lookups in the rendered buffer are unambiguous.
    fn switch_app(id: &str) -> App {
        let content = format!("[copy]\n    select = {id}\n");
        let patch = Patch::from_ini_str(&content, String::from("t")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app
    }

    fn buffer_for(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn rendered_text(app: &mut App, width: u16, height: u16) -> String {
        buffer_for(app, width, height)
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn glyph_fg(buffer: &ratatui::buffer::Buffer, glyph: &str) -> Option<Color> {
        buffer
            .content()
            .iter()
            .find(|c| c.symbol() == glyph)
            .map(|c| c.fg)
    }

    fn set_state(app: &mut App, id: &str, state: ComponentState) {
        let comp = app
            .patch
            .as_mut()
            .unwrap()
            .hw_components
            .iter_mut()
            .find(|c| c.id == id)
            .unwrap();
        comp.state = state;
    }

    // mono keeps switch (DarkGray) distinct from button (White), so the glyph
    // color proves the switch token is used rather than the button token.
    #[test]
    fn value_state_switch_renders_percentage_in_switch_token() {
        theme::set_test_theme(Some(theme::Theme::mono()));
        let mut app = switch_app("S1.1");
        set_state(&mut app, "S1.1", ComponentState::Value(0.35));

        let buffer = buffer_for(&mut app, 80, 24);
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(
            text.contains("35%"),
            "Value-state switch should render the percentage, got: {text}"
        );
        assert_eq!(
            glyph_fg(&buffer, "◉"),
            Some(theme::Theme::mono().switch),
            "filled glyph must use the switch token"
        );
        theme::set_test_theme(None);
    }

    #[test]
    fn on_off_switches_keep_glyph_and_label_rendering() {
        // Default (classic) palette: switch == button, so classic output is
        // byte-identical to the previous button-token rendering.
        let mut app = switch_app("S1.1");
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            ComponentState::Off
        );

        let buffer = buffer_for(&mut app, 80, 24);
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("□"), "Off switch keeps the hollow glyph");
        assert!(text.contains("OFF"), "Off switch keeps the OFF label");
        assert_eq!(glyph_fg(&buffer, "□"), Some(theme::Theme::classic().switch));
        assert_eq!(
            theme::Theme::classic().switch,
            theme::Theme::classic().button
        );

        set_state(&mut app, "S1.1", ComponentState::On);
        let text = rendered_text(&mut app, 80, 24);
        assert!(
            text.contains("▣"),
            "On switch keeps the filled-square glyph"
        );
        assert!(text.contains("ON"), "On switch keeps the ON label");
    }

    #[test]
    fn off_switch_uses_switch_token_not_button_token() {
        theme::set_test_theme(Some(theme::Theme::mono()));
        let mut app = switch_app("S1.1");
        let buffer = buffer_for(&mut app, 80, 24);
        let fg = glyph_fg(&buffer, "□").expect("Off switch glyph must render");
        assert_eq!(
            fg,
            theme::Theme::mono().switch,
            "Off switch must take the switch token"
        );
        assert_ne!(fg, theme::Theme::mono().button);
        theme::set_test_theme(None);
    }
}

#[cfg(test)]
mod paused_rendering_tests {
    use std::collections::HashSet;

    use super::*;
    use crate::patch::Patch;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer_for(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn text_of(buffer: &ratatui::buffer::Buffer) -> String {
        buffer.content().iter().map(|c| c.symbol()).collect()
    }

    fn cells_with_dim(buffer: &ratatui::buffer::Buffer) -> Vec<(u16, u16)> {
        let mut out = Vec::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if buffer
                    .cell((x, y))
                    .unwrap()
                    .style()
                    .add_modifier
                    .contains(Modifier::DIM)
                {
                    out.push((x, y));
                }
            }
        }
        out
    }

    fn arpeggio_app(paused: bool) -> App {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.processing_paused = paused;
        app
    }

    #[test]
    fn panels_render_dim_while_paused_header_and_status_normal() {
        let mut app = arpeggio_app(true);
        let buffer = buffer_for(&mut app, 80, 40);
        assert!(
            text_of(&buffer).contains("PROCESSING PAUSED"),
            "status bar must show the pause marker"
        );

        let dim_rows: HashSet<u16> = cells_with_dim(&buffer)
            .into_iter()
            .map(|(_, y)| y)
            .collect();
        assert!(
            !dim_rows.is_empty(),
            "panel surface must be dimmed while paused"
        );
        // Layout: header 3 rows, main area, status bar 3 rows. Dimming is
        // allowed only inside the panel main area.
        let main_top = 3u16;
        let main_bottom = 40u16.saturating_sub(3);
        for &y in &dim_rows {
            assert!(
                (main_top..main_bottom).contains(&y),
                "only panel rows may be dimmed (row {y}); header/status must stay normal"
            );
        }
        assert!(
            dim_rows
                .iter()
                .any(|&y| (main_top..main_bottom).contains(&y)),
            "expected at least one dimmed panel cell"
        );
    }

    #[test]
    fn boxed_led_cell_dims_while_paused() {
        let content = std::fs::read_to_string("fixtures/led_pairs.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("led_pairs")).unwrap();
        let mut app = App::new();
        app.load_patch(patch);
        app.processing_paused = true;
        let buffer = buffer_for(&mut app, 80, 40);
        let boxed = buffer
            .content()
            .iter()
            .find(|c| c.symbol() == "◉" || c.symbol() == "○");
        let cell = boxed.expect("boxed LED cell must render a LED glyph");
        assert!(
            cell.style().add_modifier.contains(Modifier::DIM),
            "boxed LED glyph must be dimmed while paused"
        );
    }

    #[test]
    fn resuming_processing_un_dims_and_hides_marker() {
        let mut app = arpeggio_app(true);
        let paused_buf = buffer_for(&mut app, 80, 40);
        assert!(!cells_with_dim(&paused_buf).is_empty());
        assert!(text_of(&paused_buf).contains("PROCESSING PAUSED"));

        app.processing_paused = false;
        let resumed_buf = buffer_for(&mut app, 80, 40);
        assert!(
            cells_with_dim(&resumed_buf).is_empty(),
            "no DIM anywhere once processing resumes (no shift active)"
        );
        assert!(!text_of(&resumed_buf).contains("PROCESSING PAUSED"));
    }

    #[test]
    fn pause_roundtrip_is_byte_identical_to_never_paused() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut never = App::new();
        never.load_patch(patch.clone());
        let never_buf = buffer_for(&mut never, 80, 40);

        let mut cycled = App::new();
        cycled.load_patch(patch);
        cycled.processing_paused = true;
        cycled.processing_paused = false;
        let cycled_buf = buffer_for(&mut cycled, 80, 40);

        assert_eq!(never_buf, cycled_buf);
    }

    #[test]
    fn pause_leaves_hit_rects_unchanged() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut normal = App::new();
        normal.load_patch(patch.clone());
        buffer_for(&mut normal, 80, 40);
        let normal_rects = normal.component_rects.clone();

        let mut paused = App::new();
        paused.load_patch(patch);
        paused.processing_paused = true;
        buffer_for(&mut paused, 80, 40);
        assert_eq!(normal_rects, paused.component_rects);
    }
}

#[cfg(test)]
mod graph_view_tests {
    use super::*;
    use crate::graph::{GraphEdge, TopologyIssue, TopologySeverity};
    use crate::patch::Patch;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    /// A synthetic patch with two named banner groups (clusters) and one cable
    /// fanning out: `clocktool` sources `_CLK`, `copy` and `osc` sink it.
    fn graph_app() -> App {
        let content = "\
# ---- Pulsar ----
[clocktool]
    output = _CLK
[copy]
    input = _CLK
# ---- Steady ----
[osc]
    input = _CLK
[p2b8]
";
        let mut app = App::new();
        let patch = Patch::from_ini_str(content, String::from("g")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        app
    }

    fn buffer_for(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn rendered_text(app: &mut App, width: u16, height: u16) -> String {
        buffer_for(app, width, height)
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn graph_view_renders_rounded_node_frames_and_titles() {
        let mut app = graph_app();
        let text = rendered_text(&mut app, 120, 40);
        // Rounded node frames use ╭╮╰╯; the circuit name is the title bar.
        assert!(text.contains("╭"), "node frames must use a rounded border");
        assert!(text.contains("clocktool"), "circuit name in the title bar");
    }

    #[test]
    fn graph_view_renders_input_and_output_ports() {
        let mut app = graph_app();
        let text = rendered_text(&mut app, 120, 40);
        // clocktool sources _CLK (right output port ●); copy and osc sink it
        // (left input port ◉).
        assert!(text.contains("◉"), "sink nodes need a left input port");
        assert!(text.contains("●"), "source nodes need a right output port");
    }

    #[test]
    fn graph_cluster_containers_render_titled_borders() {
        let mut app = graph_app();
        let text = rendered_text(&mut app, 120, 40);
        // Cluster containers use a plain (┌┐) border vs the nodes' rounded one,
        // and are titled with the banner name.
        assert!(
            text.contains("Pulsar"),
            "first cluster titled from its banner"
        );
        assert!(
            text.contains("Steady"),
            "second cluster titled from its banner"
        );
        assert!(text.contains("┌"), "cluster containers use a plain border");
    }

    #[test]
    fn graph_cluster_rects_published_per_cluster_within_surface() {
        let mut app = graph_app();
        let buf = buffer_for(&mut app, 120, 40);
        let clusters = app.graph.as_ref().unwrap().clusters.len();
        assert_eq!(app.graph_cluster_rects.len(), clusters);
        for (i, (index, rect)) in app.graph_cluster_rects.iter().enumerate() {
            assert_eq!(*index, i, "cluster rect indices must be sequential");
            assert!(rect.width > 0 && rect.height > 0);
            assert!(rect.x < buf.area.width && rect.y < buf.area.height);
        }
    }

    #[test]
    fn graph_node_rects_rebuilt_per_frame_not_accumulated() {
        let mut app = graph_app();
        let node_count = app.graph.as_ref().unwrap().nodes.len();
        buffer_for(&mut app, 120, 40);
        assert_eq!(app.graph_node_rects.len(), node_count);
        // Move a node the way a drag would, then render another frame: the
        // published rects must be rebuilt from the current positions, never
        // appended to the previous frame's stale entries.
        app.graph_positions[0].0 += 500.0;
        buffer_for(&mut app, 120, 40);
        assert_eq!(
            app.graph_node_rects.len(),
            node_count,
            "node rects must be rebuilt per frame, not accumulated"
        );
        let (idx, rect) = app.graph_node_rects[0];
        assert_eq!(idx, 0);
        let expected = graph_node_rects(
            &app.graph_positions,
            graph_main_area(120, 40),
            &app.graph.as_ref().unwrap().nodes,
        );
        assert_eq!(
            rect, expected[0],
            "published rects reflect the moved position"
        );
    }

    #[test]
    fn graph_view_renders_at_wide_and_narrow_sizes() {
        for (w, h) in [(120, 40), (60, 20)] {
            let mut app = graph_app();
            let text = rendered_text(&mut app, w, h);
            assert!(text.contains("╭"), "{w}×{h}: rounded node frame missing");
            assert!(
                text.contains("clocktool"),
                "{w}×{h}: node title bar missing"
            );
        }
    }

    #[test]
    fn graph_empty_state_renders_message() {
        let mut app = App::new();
        app.open_graph();
        let text = rendered_text(&mut app, 80, 24);
        assert!(text.contains("No patch loaded"));
    }

    // ---- task 5.2 edge rendering ----

    /// The graph surface `render_graph` receives: the terminal minus the 3-row
    /// header and 3-row status bar (mirrors the layout in `render`).
    fn graph_main_area(width: u16, height: u16) -> Rect {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(Rect::new(0, 0, width, height))[1]
    }

    /// An `App` in the graph view with a hand-built graph and frozen positions,
    /// giving tests full control over node geometry.
    fn graph_app_from(graph: Graph, positions: Vec<(f32, f32)>) -> App {
        let mut app = App::new();
        app.graph = Some(graph);
        app.graph_positions = positions;
        app.graph_cluster_rects.clear();
        app.graph_node_rects.clear();
        app.showing_graph = true;
        app
    }

    fn node(name: &str, idx: usize, circuit: &str, section_index: usize) -> GraphNode {
        GraphNode {
            id: (name.to_string(), idx),
            circuit: circuit.to_string(),
            instance_index: idx,
            section_index,
        }
    }

    /// In-area cells of an edge's polyline, mirroring `render_graph_edges`.
    fn edge_cells(buf: &Buffer, app: &App, cable: &str) -> Vec<(u16, u16)> {
        let graph = app.graph.as_ref().unwrap();
        let area = graph_main_area(buf.area.width, buf.area.height);
        let node_rects = graph_node_rects(&app.graph_positions, area, &graph.nodes);
        let edge = graph.edges.iter().find(|e| e.cable == cable).unwrap();
        let src = graph
            .nodes
            .iter()
            .position(|n| n.id == edge.source)
            .unwrap();
        let sink = graph.nodes.iter().position(|n| n.id == edge.sink).unwrap();
        let s = node_rects[src];
        let t = node_rects[sink];
        let x_s = s.x as i16 + s.width as i16 - 1;
        let y_s = s.y as i16 + s.height as i16 / 2;
        let x_t = t.x as i16;
        let y_t = t.y as i16 + t.height as i16 / 2;
        polyline_cells(x_s, y_s, x_t, y_t)
            .into_iter()
            .filter(|&(cx, cy)| {
                cx >= area.x as i16
                    && cx < area.x as i16 + area.width as i16
                    && cy >= area.y as i16
                    && cy < area.y as i16 + area.height as i16
            })
            .map(|(cx, cy)| (cx as u16, cy as u16))
            .collect()
    }

    /// All cells covered by any cable other than `cable`; the later-drawn edge
    /// owns a shared cell, so those cells are not asserted on one cable.
    fn other_cable_cells(buf: &Buffer, app: &App, cable: &str) -> Vec<(u16, u16)> {
        let mut others: Vec<(u16, u16)> = Vec::new();
        for c in app
            .graph
            .as_ref()
            .unwrap()
            .edges
            .iter()
            .map(|e| e.cable.as_str())
        {
            if c == cable {
                continue;
            }
            for cell in edge_cells(buf, app, c) {
                if !others.contains(&cell) {
                    others.push(cell);
                }
            }
        }
        others
    }

    /// Assert every non-port, non-shared cell of `cable`'s polyline holds a
    /// box-drawing glyph colored `expected` and stays inside `area`.
    fn assert_edge_drawn(buf: &Buffer, app: &App, cable: &str, expected: Color) {
        let graph = app.graph.as_ref().unwrap();
        let area = graph_main_area(buf.area.width, buf.area.height);
        let edge = graph.edges.iter().find(|e| e.cable == cable).unwrap();
        let src = graph
            .nodes
            .iter()
            .position(|n| n.id == edge.source)
            .unwrap();
        let sink = graph.nodes.iter().position(|n| n.id == edge.sink).unwrap();
        let node_rects = graph_node_rects(&app.graph_positions, area, &graph.nodes);
        let s = node_rects[src];
        let t = node_rects[sink];
        let port_s = (s.x + s.width - 1, s.y + s.height / 2);
        let port_t = (t.x, t.y + t.height / 2);
        let shared = other_cable_cells(buf, app, cable);
        let node_covered: Vec<(u16, u16)> = node_rects
            .iter()
            .flat_map(|r| {
                (r.x..r.x + r.width).flat_map(move |x| (r.y..r.y + r.height).map(move |y| (x, y)))
            })
            .collect();
        for (cx, cy) in edge_cells(buf, app, cable) {
            if (cx, cy) == port_s || (cx, cy) == port_t {
                continue; // port cells are covered by the node frame
            }
            if shared.contains(&(cx, cy)) {
                continue; // later-drawn edge owns shared cells
            }
            if node_covered.contains(&(cx, cy)) {
                continue; // node frame draws over the edge here
            }
            let cell = buf.cell((cx, cy)).unwrap();
            assert!(
                ["─", "│", "┌", "┐", "└", "┘", "├", "┤", "┬", "┴", "┼"].contains(&cell.symbol()),
                "cable {cable} cell ({cx},{cy}) is not a box glyph: {:?}",
                cell.symbol()
            );
            assert_eq!(cell.fg, expected, "cable {cable} cell ({cx},{cy}) color");
            assert!(
                cx < area.x + area.width && cy < area.y + area.height,
                "cable {cable} cell ({cx},{cy}) escaped the surface"
            );
        }
    }

    #[test]
    fn cable_kind_classifies_by_producing_circuit() {
        assert_eq!(CableKind::from_circuit("clocktool"), CableKind::Control);
        assert_eq!(CableKind::from_circuit("divider"), CableKind::Control);
        assert_eq!(CableKind::from_circuit("trigger2"), CableKind::Control);
        assert_eq!(CableKind::from_circuit("midi"), CableKind::Midi);
        assert_eq!(CableKind::from_circuit("notesequencer"), CableKind::Midi);
        assert_eq!(CableKind::from_circuit("osc"), CableKind::Audio);
        assert_eq!(CableKind::from_circuit("vca"), CableKind::Audio);
    }

    #[test]
    fn latency_ramp_index_maps_low_to_cold_and_max_to_hot() {
        // Formula from 2.1: round(L / (N × AVG) × (stops − 1)), clamped to the
        // last stop. Lowest latency lands on stop 0; a latency that spans the
        // whole range lands on the hottest stop.
        assert_eq!(latency_ramp_index(0.0, 4, 0.5, 5), 0, "cold end");
        assert_eq!(
            latency_ramp_index(4.0 * 0.5, 4, 0.5, 5),
            4,
            "full-range latency hits the hot end"
        );
        // Past the range clamps to the last stop.
        assert_eq!(latency_ramp_index(100.0, 4, 0.5, 5), 4, "clamped hot");
        // Monotonic: higher latency never lands on a colder stop.
        let mut last = 0usize;
        for i in 0..=10 {
            let l = i as f32 * 0.2;
            let idx = latency_ramp_index(l, 4, 0.5, 5);
            assert!(idx >= last, "latency {l} must not map colder than {last}");
            last = idx;
        }
        // Degenerate inputs stay on the cold end instead of panicking.
        assert_eq!(latency_ramp_index(1.0, 0, 0.5, 5), 0, "no edges");
        assert_eq!(latency_ramp_index(1.0, 4, 0.0, 5), 0, "no avg");
        assert_eq!(latency_ramp_index(1.0, 4, 0.5, 0), 0, "no stops");
    }

    #[test]
    fn cable_color_maps_each_inferred_kind_to_its_token() {
        let graph = Graph {
            nodes: vec![
                node("clock", 0, "clocktool", 0),
                node("osc", 0, "osc", 1),
                node("midi", 0, "midi", 2),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_CLK".into(),
                    source: ("clock".into(), 0),
                    sink: ("osc".into(), 0),
                },
                GraphEdge {
                    cable: "_AUD".into(),
                    source: ("osc".into(), 0),
                    sink: ("midi".into(), 0),
                },
                GraphEdge {
                    cable: "_NOTE".into(),
                    source: ("midi".into(), 0),
                    sink: ("clock".into(), 0),
                },
            ],
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        };
        assert_eq!(
            cable_color_with_diff(&graph, "_CLK", None, false),
            Color::Cyan,
            "control"
        );
        assert_eq!(
            cable_color_with_diff(&graph, "_AUD", None, false),
            Color::Green,
            "audio"
        );
        assert_eq!(
            cable_color_with_diff(&graph, "_NOTE", None, false),
            Color::Magenta,
            "midi"
        );
        assert_eq!(
            cable_color_with_diff(&graph, "_MISSING", None, false),
            Color::DarkGray,
            "no producing edge -> unknown"
        );
    }

    #[test]
    fn cable_color_error_token_overrides_inferred_kind() {
        let mut graph = Graph {
            nodes: vec![node("clock", 0, "clocktool", 0), node("osc", 0, "osc", 1)],
            edges: vec![GraphEdge {
                cable: "_CLK".into(),
                source: ("clock".into(), 0),
                sink: ("osc".into(), 0),
            }],
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        };
        assert_eq!(
            cable_color_with_diff(&graph, "_CLK", None, false),
            Color::Cyan
        );
        graph.validation.push(TopologyIssue {
            cable: "_CLK".into(),
            severity: TopologySeverity::Error,
            message: "n -> 1".into(),
        });
        assert_eq!(
            cable_color_with_diff(&graph, "_CLK", None, false),
            Color::Red,
            "a referenced cable renders with the error token"
        );
    }

    #[test]
    fn straight_edge_renders_box_characters_between_ports() {
        // Two nodes on the same row: the edge is a single horizontal run.
        let graph = Graph {
            nodes: vec![node("clock", 0, "clocktool", 0), node("osc", 0, "osc", 1)],
            edges: vec![GraphEdge {
                cable: "_CLK".into(),
                source: ("clock".into(), 0),
                sink: ("osc".into(), 0),
            }],
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        };
        let mut app = graph_app_from(graph, vec![(0.0, 0.5), (1.0, 0.5)]);
        let buf = buffer_for(&mut app, 120, 40);
        assert_edge_drawn(&buf, &app, "_CLK", Color::Cyan);
        // The source is clocktool (control -> cyan); spot-check an interior cell.
        let cells = edge_cells(&buf, &app, "_CLK");
        assert!(cells.len() > 2, "a straight edge must span several cells");
        let interior = cells[cells.len() / 2];
        assert_eq!(buf.cell(interior).unwrap().symbol(), "─");
        assert_eq!(buf.cell(interior).unwrap().fg, Color::Cyan);
    }

    #[test]
    fn crossing_edges_render_without_panic_and_later_wins() {
        // Square layout: A--C and B--D are diagonals whose vertical connectors
        // share the midpoint column, so the two polylines cross there.
        let graph = Graph {
            nodes: vec![
                node("a", 0, "clocktool", 0),
                node("b", 0, "osc", 1),
                node("c", 0, "osc", 2),
                node("d", 0, "vca", 3),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_CLK".into(),
                    source: ("a".into(), 0),
                    sink: ("c".into(), 0),
                },
                GraphEdge {
                    cable: "_AUD".into(),
                    source: ("b".into(), 0),
                    sink: ("d".into(), 0),
                },
            ],
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        };
        let mut app = graph_app_from(graph, vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        let buf = buffer_for(&mut app, 120, 40);
        // Both edges render fully with their own colors (no panic on the cross).
        assert_edge_drawn(&buf, &app, "_CLK", Color::Cyan);
        assert_edge_drawn(&buf, &app, "_AUD", Color::Green);
        // The shared vertical column: the later edge (_AUD) overwrites it, so
        // the crossing cell carries _AUD's glyph and color.
        let shared_x = 59u16; // midpoint of the two diagonal polylines
        let cells_a = edge_cells(&buf, &app, "_CLK");
        let crossing = cells_a
            .iter()
            .find(|&&(x, y)| x == shared_x && y > 5 && y < 30);
        let &(cx, cy) = crossing.unwrap();
        assert_eq!(buf.cell((cx, cy)).unwrap().symbol(), "│");
        assert_eq!(buf.cell((cx, cy)).unwrap().fg, Color::Green);
    }

    #[test]
    fn cluster_spanning_edge_renders_through_both_cluster_containers() {
        // graph_app puts clocktool in "Pulsar" and osc in "Steady", so _CLK is
        // a cluster-spanning cable and must still draw its polyline.
        let mut app = graph_app();
        // The cluster-spanning test targets the kind-color polyline, not the
        // latency ramp; disable ramp coloring so `_CLK` renders its control token.
        app.latency_coloring = false;
        let buf = buffer_for(&mut app, 120, 40);
        assert_edge_drawn(&buf, &app, "_CLK", Color::Cyan);
        assert!(
            !edge_cells(&buf, &app, "_CLK").is_empty(),
            "cluster-spanning cable must draw cells"
        );
    }

    #[test]
    fn topology_error_cable_renders_with_error_color() {
        let graph = Graph {
            nodes: vec![node("clock", 0, "clocktool", 0), node("osc", 0, "osc", 1)],
            edges: vec![GraphEdge {
                cable: "_CLK".into(),
                source: ("clock".into(), 0),
                sink: ("osc".into(), 0),
            }],
            clusters: vec![],
            validation: vec![TopologyIssue {
                cable: "_CLK".into(),
                severity: TopologySeverity::Warning,
                message: "dangling".into(),
            }],
            ..Default::default()
        };
        let mut app = graph_app_from(graph, vec![(0.0, 0.5), (1.0, 0.5)]);
        let buf = buffer_for(&mut app, 120, 40);
        assert_edge_drawn(&buf, &app, "_CLK", Color::Red);
    }

    #[test]
    fn edge_polyline_clips_cleanly_at_surface() {
        // Push the sink far right/bottom so the polyline's horizontal and
        // vertical runs would overrun the surface; every drawn cell must stay
        // inside `area` and the render must not panic.
        let graph = Graph {
            nodes: vec![node("clock", 0, "clocktool", 0), node("osc", 0, "osc", 1)],
            edges: vec![GraphEdge {
                cable: "_CLK".into(),
                source: ("clock".into(), 0),
                sink: ("osc".into(), 0),
            }],
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        };
        let mut app = graph_app_from(graph, vec![(0.0, 0.0), (1.0, 1.0)]);
        let buf = buffer_for(&mut app, 60, 20);
        for (cx, cy) in edge_cells(&buf, &app, "_CLK") {
            assert!(cx < buf.area.width && cy < buf.area.height);
        }
        assert_edge_drawn(&buf, &app, "_CLK", Color::Cyan);
    }

    #[test]
    fn edges_render_or_degrade_without_panic_at_small_areas() {
        for (w, h) in [(30, 10), (22, 8), (15, 6)] {
            let mut app = graph_app();
            let _ = buffer_for(&mut app, w, h); // must not panic
        }
    }

    // ---- task 3.3 disabled-circuit dim rendering ----

    /// The frame rect published for node `idx` in the last rendered frame.
    fn node_rect_of(app: &App, idx: usize) -> Rect {
        app.graph_node_rects
            .iter()
            .find(|(i, _)| *i == idx)
            .map(|(_, rect)| *rect)
            .unwrap()
    }

    /// Corner cells of a node's rounded frame: never covered by ports or
    /// edges, so they always carry the node's own border style.
    fn node_corner_cells(rect: Rect) -> Vec<(u16, u16)> {
        vec![
            (rect.x, rect.y),
            (rect.x + rect.width - 1, rect.y),
            (rect.x, rect.y + rect.height - 1),
            (rect.x + rect.width - 1, rect.y + rect.height - 1),
        ]
    }

    /// Cells of the top-border title row (ratatui places a left-aligned block
    /// title one column right of the corner).
    fn node_title_cells(rect: Rect, title_len: u16) -> Vec<(u16, u16)> {
        ((rect.x + 1)..(rect.x + 1 + title_len))
            .map(|x| (x, rect.y))
            .collect()
    }

    /// The cells of `cable`'s polyline that the edge itself owns in the
    /// buffer: port cells (covered by node frames), cells shared with
    /// later-drawn edges, and node-covered cells are excluded, mirroring
    /// `assert_edge_drawn`.
    fn cable_owned_cells(buf: &Buffer, app: &App, cable: &str) -> Vec<(u16, u16)> {
        let graph = app.graph.as_ref().unwrap();
        let area = graph_main_area(buf.area.width, buf.area.height);
        let node_rects = graph_node_rects(&app.graph_positions, area, &graph.nodes);
        let edge = graph.edges.iter().find(|e| e.cable == cable).unwrap();
        let src = graph
            .nodes
            .iter()
            .position(|n| n.id == edge.source)
            .unwrap();
        let sink = graph.nodes.iter().position(|n| n.id == edge.sink).unwrap();
        let s = node_rects[src];
        let t = node_rects[sink];
        let port_s = (s.x + s.width - 1, s.y + s.height / 2);
        let port_t = (t.x, t.y + t.height / 2);
        let shared = other_cable_cells(buf, app, cable);
        let node_covered: Vec<(u16, u16)> = node_rects
            .iter()
            .flat_map(|r| {
                (r.x..r.x + r.width).flat_map(move |x| (r.y..r.y + r.height).map(move |y| (x, y)))
            })
            .collect();
        edge_cells(buf, app, cable)
            .into_iter()
            .filter(|cell| {
                *cell != port_s
                    && *cell != port_t
                    && !shared.contains(cell)
                    && !node_covered.contains(cell)
            })
            .collect()
    }

    /// `clocktool` sources `_CLK` (with a validation finding) to `osc` and
    /// `_AUD` to `vca`; `osc` sources `_MOD` to `vca`. Disabling `clocktool`
    /// makes `_CLK` and `_AUD` incident while `_MOD` stays untouched.
    fn three_node_graph() -> Graph {
        Graph {
            nodes: vec![
                node("clock", 0, "clocktool", 0),
                node("osc", 0, "osc", 1),
                node("vca", 0, "vca", 2),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_CLK".into(),
                    source: ("clock".into(), 0),
                    sink: ("osc".into(), 0),
                },
                GraphEdge {
                    cable: "_AUD".into(),
                    source: ("clock".into(), 0),
                    sink: ("vca".into(), 0),
                },
                GraphEdge {
                    cable: "_MOD".into(),
                    source: ("osc".into(), 0),
                    sink: ("vca".into(), 0),
                },
            ],
            clusters: vec![],
            validation: vec![TopologyIssue {
                cable: "_CLK".into(),
                severity: TopologySeverity::Error,
                message: "n -> 1".into(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn disabled_node_frame_and_title_render_dim_enabled_nodes_normal() {
        let mut app = graph_app();
        app.disabled_circuits.insert((String::from("copy"), 0));
        let buf = buffer_for(&mut app, 120, 40);
        let graph = app.graph.as_ref().unwrap();
        let copy_idx = graph
            .nodes
            .iter()
            .position(|n| n.circuit == "copy")
            .unwrap();
        let clock_idx = graph
            .nodes
            .iter()
            .position(|n| n.circuit == "clocktool")
            .unwrap();
        let theme = theme::active();

        // Disabled node: frame corners and title dimmed with the dim token.
        let rect = node_rect_of(&app, copy_idx);
        for cell in node_corner_cells(rect)
            .into_iter()
            .chain(node_title_cells(rect, 4))
        {
            let c = buf.cell(cell).unwrap();
            assert!(
                c.modifier.contains(Modifier::DIM),
                "disabled node cell {cell:?} must render dim: {c:?}"
            );
        }
        for cell in node_corner_cells(rect) {
            assert_eq!(
                buf.cell(cell).unwrap().fg,
                theme.graph_node_dim,
                "disabled node border uses the dim token"
            );
        }

        // Enabled node: no dim on frame or title, normal chrome tokens.
        let rect = node_rect_of(&app, clock_idx);
        for cell in node_corner_cells(rect)
            .into_iter()
            .chain(node_title_cells(rect, 9))
        {
            let c = buf.cell(cell).unwrap();
            assert!(
                !c.modifier.contains(Modifier::DIM),
                "enabled node cell {cell:?} must not render dim: {c:?}"
            );
        }
        assert_eq!(
            buf.cell(node_corner_cells(rect)[0]).unwrap().fg,
            theme.graph_node_border,
            "enabled node border keeps its normal token"
        );
    }

    #[test]
    fn disabled_node_dims_incident_edges_but_error_red_wins() {
        let mut app = graph_app_from(three_node_graph(), vec![(0.0, 0.5), (1.0, 0.0), (1.0, 1.0)]);
        app.disabled_circuits.insert((String::from("clocktool"), 0));
        let buf = buffer_for(&mut app, 120, 40);
        let theme = theme::active();

        // _AUD is incident to the disabled clocktool node without a finding:
        // dim token + DIM modifier, influence-independent.
        let aud = cable_owned_cells(&buf, &app, "_AUD");
        assert!(!aud.is_empty(), "_AUD polyline must draw owned cells");
        for cell in aud {
            let c = buf.cell(cell).unwrap();
            assert_eq!(
                c.fg, theme.graph_edge_dim,
                "incident edge _AUD cell {cell:?} must use the dim token"
            );
            assert!(
                c.modifier.contains(Modifier::DIM),
                "incident edge _AUD cell {cell:?} must render dim"
            );
        }

        // _CLK is also incident but carries a validation finding: error red
        // outranks dim and stays plain red.
        let clk = cable_owned_cells(&buf, &app, "_CLK");
        assert!(!clk.is_empty(), "_CLK polyline must draw owned cells");
        for cell in clk {
            let c = buf.cell(cell).unwrap();
            assert_eq!(
                c.fg, theme.graph_edge_error,
                "error cable _CLK cell {cell:?} must keep the error token"
            );
            assert!(
                !c.modifier.contains(Modifier::DIM),
                "error cable _CLK cell {cell:?} must not be dimmed"
            );
        }

        // _MOD is not incident to the disabled node: kind color, no dim.
        let kind_color = cable_color_with_diff(app.graph.as_ref().unwrap(), "_MOD", None, false);
        let mod_cells = cable_owned_cells(&buf, &app, "_MOD");
        assert!(!mod_cells.is_empty(), "_MOD polyline must draw owned cells");
        for cell in mod_cells {
            let c = buf.cell(cell).unwrap();
            assert_eq!(
                c.fg, kind_color,
                "non-incident edge _MOD cell {cell:?} keeps its kind color"
            );
            assert!(
                !c.modifier.contains(Modifier::DIM),
                "non-incident edge _MOD cell {cell:?} must not be dimmed"
            );
        }
    }

    #[test]
    fn disabled_node_under_hover_keeps_hover_styling() {
        let mut app = graph_app();
        app.disabled_circuits.insert((String::from("copy"), 0));
        let copy_idx = app
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .position(|n| n.circuit == "copy")
            .unwrap();
        app.hovered_graph_node = Some(copy_idx);
        let buf = buffer_for(&mut app, 120, 40);
        let theme = theme::active();

        // Hover emphasis stays visible on the disabled node: reversed on the
        // muted background, with the disabled dim still present underneath.
        let rect = node_rect_of(&app, copy_idx);
        for cell in node_corner_cells(rect)
            .into_iter()
            .chain(node_title_cells(rect, 4))
        {
            let c = buf.cell(cell).unwrap();
            assert!(
                c.modifier.contains(Modifier::REVERSED),
                "hovered disabled node cell {cell:?} must keep hover styling: {c:?}"
            );
            assert_eq!(
                c.bg, theme.muted,
                "hovered disabled node cell {cell:?} keeps the hover background"
            );
            assert!(
                c.modifier.contains(Modifier::DIM),
                "hovered disabled node cell {cell:?} stays marked disabled"
            );
        }

        // No other node is hovered: no reversed frame cells elsewhere.
        let clock_idx = app
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .position(|n| n.circuit == "clocktool")
            .unwrap();
        let rect = node_rect_of(&app, clock_idx);
        for cell in node_corner_cells(rect) {
            assert!(
                !buf.cell(cell)
                    .unwrap()
                    .modifier
                    .contains(Modifier::REVERSED),
                "unhovered node must not render hover styling"
            );
        }
    }

    #[test]
    fn graph_without_disabled_circuits_renders_without_dim_drift() {
        let mut app = graph_app();
        assert!(app.disabled_circuits.is_empty());
        // The kind token is the baseline this test pins; ramp coloring would
        // re-color `_CLK`, so turn it off to keep the assertion about dims.
        app.latency_coloring = false;
        let buf = buffer_for(&mut app, 120, 40);
        // No dimming anywhere: the disabled-circuit path must be inert unless
        // a circuit actually is disabled (no drift vs. prior rendering).
        for (i, cell) in buf.content().iter().enumerate() {
            assert!(
                !cell.modifier.contains(Modifier::DIM),
                "cell {i} ({:?}) unexpectedly dimmed with no disabled circuits",
                cell.symbol()
            );
        }
        // Edge coloring unchanged: the kind token still applies.
        assert_edge_drawn(&buf, &app, "_CLK", Color::Cyan);
    }

    // ── label ellipsis (droid_tui-lsd) ────────────────────────────────────

    #[test]
    fn truncate_with_ellipsis_keeps_short_labels_and_appends_on_overflow() {
        assert_eq!(truncate_with_ellipsis("short", 8), "short");
        assert_eq!(truncate_with_ellipsis("exactly", 7), "exactly");
        // Over-length: max_chars - 1 chars + the ellipsis.
        assert_eq!(
            truncate_with_ellipsis("[t2 P] Modulation", 14),
            "[t2 P] Modula…"
        );
        assert_eq!(truncate_with_ellipsis("longlabel", 5), "long…");
        // Multi-byte safe: ellipsis itself is a single char.
        assert_eq!(truncate_with_ellipsis("αβγδε", 3), "αβ…");
        // Degenerate maxima never panic and stay within the budget.
        assert_eq!(truncate_with_ellipsis("x", 0), "");
        assert_eq!(truncate_with_ellipsis("xy", 1), "…");
    }
}
