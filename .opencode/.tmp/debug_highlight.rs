use droid_tui::app::{App, SourceViewMode, ViewerFocus};
use droid_tui::patch::Patch;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

fn main() {
    let content = std::fs::read_to_string("fixtures/source_navigation.ini").unwrap();
    let patch = Patch::from_ini_str(&content, "source_navigation".to_string()).unwrap();
    println!("raw_lines {}", patch.raw_lines.len());
    for (i, l) in patch.raw_lines.iter().enumerate() {
        println!("{:02}: {}", i, l);
    }
    println!("--- occurrence B1.1 ---");
    for s in patch.occurrences_for("B1.1") {
        println!(
            "line {} col {}-{} raw='{}'",
            s.line,
            s.col_start,
            s.col_end,
            &patch.raw_lines[s.line][s.col_start..s.col_end]
        );
        // verify substring
    }
    println!("--- modifier B1.1 ---");
    for e in patch.modifier_entries_for("B1.1") {
        println!(
            "line {} col {}-{} source={} selectat={:?} raw='{}'",
            e.span.line,
            e.span.col_start,
            e.span.col_end,
            e.source,
            e.selectat,
            &patch.raw_lines[e.span.line][e.span.col_start..e.span.col_end]
        );
    }
    println!("--- modifier B1.2 ---");
    for e in patch.modifier_entries_for("B1.2") {
        println!(
            "line {} source={} sel {:?}",
            e.span.line, e.source, e.selectat
        );
    }
    println!("--- modifier P1.1 ---");
    for e in patch.modifier_entries_for("P1.1") {
        println!(
            "line {} source={} sel {:?}",
            e.span.line, e.source, e.selectat
        );
    }

    // Render buffer
    let mut app = App::new();
    app.load_patch(patch);
    app.showing_viewer = true;
    app.source_view_mode = SourceViewMode::Raw;
    app.viewer_focus = ViewerFocus::Source;
    app.select_component(String::from("B1.1"));
    app.source_scroll = 0;
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| droid_tui::ui::render(frame, &mut app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let area = buffer.area;
    // dump rows containing B1.1
    for y in 0..area.height {
        let mut row: String = String::new();
        let mut styles: Vec<String> = vec![];
        for x in 0..area.width {
            let cell = buffer.cell((x, y)).unwrap();
            row.push_str(cell.symbol());
            if cell.symbol() != " " {
                let fg = cell
                    .style()
                    .fg
                    .map(|c| format!("{:?}", c))
                    .unwrap_or("None".to_string());
                let mods = format!("{:?}", cell.style().add_modifier);
                styles.push(format!("x{}:{} fg={} mods={}", x, cell.symbol(), fg, mods));
            }
        }
        if row.contains("B1.1") {
            println!("y{} row='{}'", y, row);
            // print styles for that row substring
            let idx = row.find("B1.1").unwrap();
            for x in idx..idx + 4 {
                let cell = buffer.cell((x as u16, y)).unwrap();
                println!(
                    "  x{} '{}' fg={:?} add_mod={:?} bg={:?}",
                    x,
                    cell.symbol(),
                    cell.style().fg,
                    cell.style().add_modifier,
                    cell.style().bg
                );
            }
        }
        if row.contains("_TRANSIT") {
            println!("y{} row _TRANSIT='{}'", y, row);
            let idx = row.find("_TRANSIT").unwrap();
            for x in idx..idx + 8 {
                let cell = buffer.cell((x as u16, y)).unwrap();
                println!(
                    "  x{} '{}' fg={:?} add_mod={:?} bg={:?}",
                    x,
                    cell.symbol(),
                    cell.style().fg,
                    cell.style().add_modifier,
                    cell.style().bg
                );
            }
        }
        if row.contains("P1.1") {
            println!("y{} row P1.1='{}'", y, row);
        }
    }
    // also test has_highlighted_token logic
    let has = has_highlighted_token(&buffer, "B1.1", Some(Color::Yellow), Some(Modifier::BOLD));
    println!("has_highlighted_token B1.1 Yellow Bold = {}", has);
    let has_rev = has_highlighted_token(
        &buffer,
        "B1.1",
        Some(Color::Yellow),
        Some(Modifier::REVERSED),
    );
    println!("has B1.1 yellow reversed = {}", has_rev);
    let has_cyan = has_highlighted_token(
        &buffer,
        "B1.1",
        Some(Color::Cyan),
        Some(Modifier::UNDERLINED),
    );
    println!("has B1.1 cyan underlined = {}", has_cyan);

    // dump buffer visually
}

fn has_highlighted_token(
    buffer: &ratatui::buffer::Buffer,
    token: &str,
    want_fg: Option<Color>,
    want_modifier: Option<Modifier>,
) -> bool {
    let area = buffer.area;
    for y in 0..area.height {
        let mut row_chars: Vec<char> = Vec::new();
        let mut row_styles: Vec<ratatui::style::Style> = Vec::new();
        for x in 0..area.width {
            let cell = buffer.cell((x, y)).unwrap();
            row_chars.push(cell.symbol().chars().next().unwrap_or(' '));
            row_styles.push(cell.style());
        }
        let row_str: String = row_chars.iter().collect();
        let mut search_from = 0;
        while let Some(pos) = row_str[search_from..].find(token) {
            let start = search_from + pos;
            let mut all_match = true;
            for i in 0..token.len() {
                let idx = start + i;
                if idx >= row_styles.len() {
                    all_match = false;
                    break;
                }
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
            search_from = start + 1;
            if search_from >= row_str.len() {
                break;
            }
        }
    }
    false
}
