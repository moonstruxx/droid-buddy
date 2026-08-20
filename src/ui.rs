use std::collections::HashMap;

use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
<<<<<<< Updated upstream
use crate::patch::{ComponentKind, ComponentState, ShiftGroup};
=======
use crate::patch::{ComponentKind, ComponentState};
>>>>>>> Stashed changes

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(10),   // main content
            Constraint::Length(3), // status bar
        ])
        .split(frame.area());

<<<<<<< Updated upstream
    render_header(frame, chunks[0], app);
    render_main(frame, chunks[1], app);
    render_status(frame, chunks[2], app);
=======
        render_header(frame, chunks[0], app);
        render_main(frame, chunks[1], app);
        render_status(frame, chunks[2], app);
    }
>>>>>>> Stashed changes
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = match &app.patch {
        Some(patch) => format!(" DROID: {} ", patch.name),
        None => String::from(" DROID TUI "),
    };

    let header = Paragraph::new(title)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
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
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(msg, area);
}

const COMPONENT_WIDTH: u16 = 16;
const COMPONENT_HEIGHT: u16 = 2;

/// Render hardware components grouped into physical controller panels
/// (P2B8, Faderbank, Notebuttons, CV I/O, ...) that mirror the hardware
/// layout, wrapping components onto extra rows when a panel doesn't fit
/// the terminal width. See controller-panels/spec.md.
fn render_patch(frame: &mut Frame, area: Rect, patch: &crate::patch::Patch, app: &mut App) {
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

    let mut constraints: Vec<Constraint> = panel_order
        .iter()
        .map(|name| Constraint::Length(rows_for(panels[name].len()) * COMPONENT_HEIGHT + 2))
        .collect();
    constraints.push(Constraint::Min(0));

    let panel_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .flex(Flex::Start)
        .split(area);

    for (i, name) in panel_order.iter().enumerate() {
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
                    .fg(group.color())
                    .add_modifier(Modifier::BOLD),
                format!(" {} [SHIFT {}] ", name, group.key_label()),
            ),
            Some(_) => (
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
                format!(" {} ", name),
            ),
            None => (Style::default().fg(Color::DarkGray), format!(" {} ", name)),
        };

        let block = Block::default()
            .title(title)
            .title_style(border_style)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(panel_chunks[i]);
        frame.render_widget(block, panel_chunks[i]);

        let row_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(COMPONENT_HEIGHT);
                rows_for(components.len()) as usize
            ])
            .flex(Flex::Start)
            .split(inner);

        for (row_i, row) in components.chunks(cols).enumerate() {
            let comp_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Length(COMPONENT_WIDTH); row.len()])
                .flex(Flex::Start)
                .split(row_chunks[row_i]);

            for (col_i, comp) in row.iter().enumerate() {
                let global_idx = index_of[comp.id.as_str()];
                let is_hovered = app.hovered_component == Some(global_idx);
                let is_shift_active =
                    comp.shift_group.is_some() && comp.shift_group == app.active_shift;
                render_component(frame, comp_chunks[col_i], comp, is_hovered, is_shift_active);
                app.component_rects.push((global_idx, comp_chunks[col_i]));
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
                    Color::Yellow
                } else {
                    Color::White
                },
            )
        }
        ComponentKind::CvIn => ("→", String::from("CV IN"), Color::Cyan),
        ComponentKind::CvOut => ("←", String::from("CV OUT"), Color::Green),
        ComponentKind::Knob => {
            let val = match &comp.state {
                ComponentState::Value(v) => format!("{:.0}%", v * 100.0),
                _ => String::from("---"),
            };
            ("◉", val, Color::Magenta)
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
                Color::White,
            )
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
                Color::Red,
            )
        }
    };

    let hover_style = if is_hovered {
        Style::default()
            .fg(fg_color)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(fg_color)
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(symbol, hover_style),
            Span::raw(" "),
            Span::styled(&comp.label, hover_style),
        ]),
        Line::from(Span::styled(
            state_text,
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let widget = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::styled(
        &app.status_message,
        Style::default().fg(Color::White),
    )];

    if let Some(group) = app.active_shift {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            format!("SHIFT {} ACTIVE", group.key_label()),
            Style::default()
                .fg(group.color())
                .add_modifier(Modifier::BOLD),
        ));
    }

    let status = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::DarkGray))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    frame.render_widget(status, area);
}
<<<<<<< Updated upstream
=======

fn render_picker(frame: &mut Frame, area: Rect, app: &App) {
    // Calculate picker dimensions (70% width, 50% height, centered)
    let picker_width = (area.width.saturating_sub(4)).max(40);
    let picker_height = (area.height.saturating_sub(4)).max(20);
    let picker_x = area.x + (area.width - picker_width) / 2;
    let picker_y = area.y + (area.height - picker_height) / 2;
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
        .style(Style::default().bg(Color::DarkGray))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" File Picker ")
                .border_style(Style::default().fg(Color::Blue)),
        );

    frame.render_widget(paragraph, picker_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
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
}
>>>>>>> Stashed changes
