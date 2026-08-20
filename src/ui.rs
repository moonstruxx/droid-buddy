use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::patch::{ComponentKind, ComponentState, ShiftGroup};

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(10),   // main content
            Constraint::Length(3), // status bar
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    render_main(frame, chunks[1], app);
    render_status(frame, chunks[2], app);
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

fn render_patch(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    patch: &crate::patch::Patch,
    app: &mut App,
) {
    // Group components by shift group
    let mut groups: Vec<(ShiftGroup, Vec<&crate::patch::HwComponent>)> = Vec::new();
    let mut ungrouped: Vec<&crate::patch::HwComponent> = Vec::new();

    for comp in &patch.hw_components {
        if let Some(group) = comp.shift_group {
            if let Some(pos) = groups.iter().position(|(g, _)| *g == group) {
                groups[pos].1.push(comp);
            } else {
                groups.push((group, vec![comp]));
            }
        } else {
            ungrouped.push(comp);
        }
    }

    // Build layout: one row per group + ungrouped
    let mut constraints = Vec::new();
    for _ in &groups {
        constraints.push(Constraint::Length(5));
    }
    if !ungrouped.is_empty() {
        constraints.push(Constraint::Length(5));
    }
    constraints.push(Constraint::Min(1));

    let group_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .flex(Flex::Start)
        .split(area);

    let mut idx = 0;
    for (i, (group, components)) in groups.iter().enumerate() {
        let is_active = app.active_shift == Some(*group);
        let border_color = if is_active {
            group.color()
        } else {
            Color::DarkGray
        };

        let border_style = if is_active {
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(border_color)
        };

        let block = Block::default()
            .title(format!(" Shift {} ", group.key_label()))
            .title_style(border_style)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(group_chunks[i]);
        frame.render_widget(block, group_chunks[i]);

        // Render components inside the group
        let comp_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Length(16); components.len()])
            .flex(Flex::Start)
            .split(inner);

        for (j, comp) in components.iter().enumerate() {
            let is_hovered = app.hovered_component == Some(idx);
            render_component(frame, comp_chunks[j], comp, is_hovered, is_active);
            idx += 1;
        }
    }

    // Render ungrouped components
    if !ungrouped.is_empty() {
        let block = Block::default()
            .title(" General ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let last_idx = groups.len();
        let inner = block.inner(group_chunks[last_idx]);
        frame.render_widget(block, group_chunks[last_idx]);

        let comp_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Length(16); ungrouped.len()])
            .flex(Flex::Start)
            .split(inner);

        for (j, comp) in ungrouped.iter().enumerate() {
            let is_hovered = app.hovered_component == Some(idx);
            render_component(frame, comp_chunks[j], comp, is_hovered, false);
            idx += 1;
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
