//! Gallery generator for visual validation (tasks 2.1).
//! Renders the same coverage matrix as the `visual_*` insta tests via
//! `TestBackend` → `Buffer` → HTML (same `buffer_to_html` helper) and
//! materializes `evidence/gallery/index.html` with one row per scenario and
//! columns `classic | terminal | mono`. Also writes per-scenario ANSI files
//! for inspectability. Keep ephemeral: `evidence/gallery/` is `.gitignore`'d.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

use crate::app::{App, ViewerFocus};
use crate::handler::handle_event;
use crate::patch::ShiftGroup;
use crate::theme;
use crate::ui::render;

// ── shared helpers (same as regression.rs 1.1) ───────────────────────────

pub fn color_to_css(color: Color) -> Option<String> {
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

pub fn html_escape(s: &str) -> String {
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

pub fn buffer_to_ansi(buffer: &Buffer) -> String {
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

pub fn buffer_to_html(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut rows = Vec::with_capacity(area.height as usize);
    for y in 0..area.height {
        let mut row_html = String::new();
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

pub fn buffer_for(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// Pins a built-in palette for the calling thread; drops back to global.
pub struct ThemedGuard;

impl ThemedGuard {
    pub fn pin(name: &str) -> Self {
        theme::set_test_theme(Some(*theme::resolve(name)));
        Self
    }
}

impl Drop for ThemedGuard {
    fn drop(&mut self) {
        theme::set_test_theme(None);
    }
}

fn app_from_fixture(name: &str) -> App {
    let path = format!("fixtures/{name}.ini");
    let patch = crate::patch::Patch::from_ini_file(Path::new(&path)).unwrap();
    let mut app = App::new();
    app.load_patch(patch);
    app
}

fn led_pairs_app() -> App {
    let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/led_pairs.ini")).unwrap();
    let mut app = App::new();
    app.load_patch(patch);
    app
}

// ── gallery definition ────────────────────────────────────────────────────

struct Scenario {
    id: &'static str,
    label: &'static str,
    width: u16,
    height: u16,
    setup: fn(&mut App),
}

fn setup_arpeggio(app: &mut App) {
    let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
    app.load_patch(patch);
}

fn setup_led_pairs(app: &mut App) {
    *app = led_pairs_app();
}

fn setup_viewer_closed(app: &mut App) {
    *app = app_from_fixture("source_navigation");
    app.showing_viewer = false;
}

fn setup_viewer_open(app: &mut App) {
    *app = app_from_fixture("source_navigation");
    app.select_component(String::from("B1.1"));
    app.showing_viewer = true;
    app.viewer_focus = ViewerFocus::Source;
}

fn setup_arpeggio_shift1(app: &mut App) {
    let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
    app.load_patch(patch);
    app.active_shift = Some(ShiftGroup::Group1);
}

fn setup_led_shift1(app: &mut App) {
    *app = led_pairs_app();
    app.active_shift = Some(ShiftGroup::Group1);
}

fn setup_viewer_shift1(app: &mut App) {
    *app = app_from_fixture("source_navigation");
    app.select_component(String::from("B1.1"));
    app.showing_viewer = true;
    app.viewer_focus = ViewerFocus::Source;
    app.active_shift = Some(ShiftGroup::Group1);
}

/// Drive one key through the real handler so gallery scenarios exercise the
/// same input path as the running TUI instead of hand-set state fields.
fn press(app: &mut App, code: KeyCode) {
    handle_event(KeyEvent::new(code, KeyModifiers::NONE), app);
}

fn setup_viewer_live_shift1(app: &mut App) {
    // droid_tui-0lw: shift1 is pressed while the viewer is open and the
    // source pane is focused — inert before the fix, now live, so the frame
    // shows viewer chrome + shift chip + bold shift borders together.
    *app = app_from_fixture("source_navigation");
    app.select_component(String::from("B1.1"));
    press(app, KeyCode::Char('g'));
    press(app, KeyCode::Char('v'));
    press(app, KeyCode::Char('1'));
}

fn setup_viewer_live_toggle(app: &mut App) {
    // droid_tui-0lw: Enter toggles AND selects B1.1 while Source is focused,
    // jumping source_scroll to its first occurrence with the viewer open.
    *app = app_from_fixture("source_navigation");
    press(app, KeyCode::Char('g'));
    press(app, KeyCode::Char('v'));
    let idx = app
        .patch
        .as_ref()
        .and_then(|p| p.hw_components.iter().position(|c| c.id == "B1.1"));
    app.hovered_component = idx;
    press(app, KeyCode::Enter);
}

fn setup_quad_modifier_none(app: &mut App) {
    *app = app_from_fixture("modifier_switch_passthrough");
    app.open_quad();
}

fn setup_quad_modifier_b1(app: &mut App) {
    *app = app_from_fixture("modifier_switch_passthrough");
    app.select_component(String::from("B1.1"));
    app.open_quad();
}

fn setup_modifier_arpeggio1(app: &mut App) {
    *app = app_from_fixture("arpeggio1");
    app.select_component(String::from("B1.1"));
}

fn setup_modifier_source_navigation(app: &mut App) {
    *app = app_from_fixture("source_navigation");
    app.select_component(String::from("B1.1"));
}

fn setup_modifier_cable_banner(app: &mut App) {
    *app = app_from_fixture("cable_banner_combos");
    app.select_component(String::from("B1.1"));
}

fn setup_modifier_shift(app: &mut App) {
    *app = app_from_fixture("modifier_switch_passthrough");
    app.select_component(String::from("B1.1"));
    app.active_shift = Some(ShiftGroup::Group1);
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "arpeggio_80",
        label: "arpeggio1 · width 80 · panels only",
        width: 80,
        height: 30,
        setup: setup_arpeggio,
    },
    Scenario {
        id: "arpeggio_120",
        label: "arpeggio1 · width 120 · panels only",
        width: 120,
        height: 30,
        setup: setup_arpeggio,
    },
    Scenario {
        id: "led_pairs_100",
        label: "led_pairs · width 100 · mixed boxed/text",
        width: 100,
        height: 40,
        setup: setup_led_pairs,
    },
    Scenario {
        id: "viewer_closed_100",
        label: "source_navigation · width 100 · viewer closed",
        width: 100,
        height: 40,
        setup: setup_viewer_closed,
    },
    Scenario {
        id: "viewer_open_100",
        label: "source_navigation · width 100 · viewer open (B1.1)",
        width: 100,
        height: 40,
        setup: setup_viewer_open,
    },
    Scenario {
        id: "arpeggio_shift1_100",
        label: "arpeggio1 · width 100 · shift1 active",
        width: 100,
        height: 30,
        setup: setup_arpeggio_shift1,
    },
    Scenario {
        id: "led_shift1_100",
        label: "led_pairs · width 100 · shift1 active",
        width: 100,
        height: 40,
        setup: setup_led_shift1,
    },
    Scenario {
        id: "viewer_shift1_100",
        label: "source_navigation · width 100 · viewer open + shift1",
        width: 100,
        height: 40,
        setup: setup_viewer_shift1,
    },
    Scenario {
        id: "viewer_live_shift1_100",
        label: "source_navigation · width 100 · viewer open · shift1 pressed while Source focused",
        width: 100,
        height: 40,
        setup: setup_viewer_live_shift1,
    },
    Scenario {
        id: "viewer_live_toggle_100",
        label: "source_navigation · width 100 · viewer open · B1.1 toggled+selected via Enter",
        width: 100,
        height: 40,
        setup: setup_viewer_live_toggle,
    },
    Scenario {
        id: "quad_none_120",
        label: "modifier_switch_passthrough · width 120 · quad 4-pane · no modifier",
        width: 120,
        height: 40,
        setup: setup_quad_modifier_none,
    },
    Scenario {
        id: "quad_b1_120",
        label: "modifier_switch_passthrough · width 120 · quad 4-pane · B1.1 FULL highlight + FILTERED compact",
        width: 120,
        height: 40,
        setup: setup_quad_modifier_b1,
    },
    Scenario {
        id: "quad_b1_100",
        label: "modifier_switch_passthrough · width 100 · quad fallback · B1.1 fallback (<120)",
        width: 100,
        height: 40,
        setup: setup_quad_modifier_b1,
    },
    Scenario {
        id: "quad_b1_80",
        label: "modifier_switch_passthrough · width 80 · quad fallback · B1.1 narrow",
        width: 80,
        height: 40,
        setup: setup_quad_modifier_b1,
    },
    // ── modifier highlight matrix (task 5.1/5.2): B1.1 influence at 80/120 ─────
    Scenario {
        id: "modifier_arpeggio1_80",
        label: "arpeggio1 · width 80 · MOD B1.1 wash",
        width: 80,
        height: 40,
        setup: setup_modifier_arpeggio1,
    },
    Scenario {
        id: "modifier_arpeggio1_120",
        label: "arpeggio1 · width 120 · MOD B1.1 wash",
        width: 120,
        height: 40,
        setup: setup_modifier_arpeggio1,
    },
    Scenario {
        id: "modifier_source_navigation_80",
        label: "source_navigation · width 80 · MOD B1.1 wash",
        width: 80,
        height: 40,
        setup: setup_modifier_source_navigation,
    },
    Scenario {
        id: "modifier_source_navigation_120",
        label: "source_navigation · width 120 · MOD B1.1 wash",
        width: 120,
        height: 40,
        setup: setup_modifier_source_navigation,
    },
    Scenario {
        id: "modifier_cable_banner_80",
        label: "cable_banner_combos · width 80 · MOD B1.1 wash",
        width: 80,
        height: 40,
        setup: setup_modifier_cable_banner,
    },
    Scenario {
        id: "modifier_cable_banner_120",
        label: "cable_banner_combos · width 120 · MOD B1.1 wash",
        width: 120,
        height: 40,
        setup: setup_modifier_cable_banner,
    },
    Scenario {
        id: "modifier_shift_80",
        label: "modifier_switch_passthrough · width 80 · MOD B1.1 + SHIFT1 coexistence",
        width: 80,
        height: 40,
        setup: setup_modifier_shift,
    },
    Scenario {
        id: "modifier_shift_120",
        label: "modifier_switch_passthrough · width 120 · MOD B1.1 + SHIFT1 coexistence",
        width: 120,
        height: 40,
        setup: setup_modifier_shift,
    },
];

/// Render the whole matrix and write `evidence/gallery/index.html` plus
/// per-scenario ANSI files. Returns the HTML path.
pub fn generate_gallery() -> std::io::Result<PathBuf> {
    let out_dir = Path::new("evidence/gallery");
    fs::create_dir_all(out_dir)?;
    let out_path = out_dir.join("index.html");

    let mut rows_html = String::new();
    for scenario in SCENARIOS {
        rows_html.push_str(&format!(
            "    <tr><td class=\"scenario\">{}</td>",
            html_escape(scenario.label)
        ));
        for &theme_name in theme::THEMES {
            let _guard = ThemedGuard::pin(theme_name);
            let mut app = App::new();
            (scenario.setup)(&mut app);
            let buf = buffer_for(&mut app, scenario.width, scenario.height);
            let ansi = buffer_to_ansi(&buf);
            let html = buffer_to_html(&buf);

            // Write per-scenario ANSI for inspectability (ephemeral, gitignored)
            let ansi_path = out_dir.join(format!("{}_{}.ansi", scenario.id, theme_name));
            fs::write(&ansi_path, &ansi)?;

            // Also write raw HTML cell for debugging if needed
            let html_cell_path = out_dir.join(format!("{}_{}.html", scenario.id, theme_name));
            fs::write(&html_cell_path, &html)?;

            rows_html.push_str(&format!(
                "<td data-theme=\"{theme_name}\" data-scenario=\"{}\"><pre class=\"cell\">{html}</pre></td>",
                scenario.id
            ));
        }
        rows_html.push_str("</tr>\n");
    }

    let page = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>droid_tui visual gallery</title>
<style>
body {{ font-family: sans-serif; margin: 16px; background: #111; color: #eee; }}
h1 {{ font-size: 1.2rem; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 1px solid #333; padding: 6px; vertical-align: top; }}
th {{ background: #222; text-align: left; }}
td.scenario {{ background: #1a1a1a; font-size: 0.85rem; white-space: nowrap; max-width: 180px; }}
pre.cell {{ margin: 0; font-family: "Cascadia Mono", "Fira Code", monospace; font-size: 11px; line-height: 1.15; white-space: pre; background: #000; color: #ccc; padding: 6px; overflow-x: auto; }}
pre.cell span {{ white-space: pre; }}
.legend {{ margin: 12px 0; color: #aaa; font-size: 0.85rem; }}
</style>
</head>
<body>
<h1>droid_tui — visual gallery (TestBackend → ANSI → HTML)</h1>
<p class="legend">One row per scenario, columns <code>classic</code> / <code>terminal</code> / <code>mono</code> (widths 80/120, viewer open/closed, shift active). Each cell is the same HTML from <code>buffer_to_html</code> used in insta snapshots. Generated via <code>cargo run --bin snapshot-gallery</code> or <code>cargo test -- --generate-gallery</code>.</p>
<table>
<thead><tr><th>Scenario</th><th>classic</th><th>terminal</th><th>mono</th></tr></thead>
<tbody>
{rows_html}</tbody>
</table>
<p class="legend">Ephemeral: <code>evidence/gallery/</code> is .gitignore'd; archive is durable at <code>openspec/changes/archive/…/evidence/gallery/</code>.</p>
</body>
</html>
"#
    );

    fs::write(&out_path, page)?;
    Ok(out_path)
}

/// Whether the current test run asked for gallery generation via
/// `cargo test -- --generate-gallery` or `GENERATE_GALLERY=1`.
pub fn should_generate_gallery() -> bool {
    if std::env::var("GENERATE_GALLERY").is_ok() {
        return true;
    }
    std::env::args().any(|a| a == "--generate-gallery" || a == "generate-gallery")
}
