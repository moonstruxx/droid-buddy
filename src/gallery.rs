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
use crate::patch::{ComponentState, ShiftGroup};
use crate::rendermetrics::{score_render, RenderFeatures};
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

/// Load a fixture from `fixtures/ui_review/` (controller-front review rows).
fn app_from_ui_review(name: &str) -> App {
    let path = format!("fixtures/ui_review/{name}.ini");
    let patch = crate::patch::Patch::from_ini_file(Path::new(&path)).unwrap();
    let mut app = App::new();
    app.load_patch(patch);
    app
}

fn setup_ui_review_p4b2(app: &mut App) {
    // P4B2 physical front (5 HP): four pots (P1.1..P1.4), two buttons with
    // LEDs (B1.1/B1.2 + L1.1/L1.2). [p4b2] is registered in
    // KNOWN_CONTROLLER_SECTIONS, so the patch resolves to the real p4b2
    // geometry key (not the fallback panel).
    *app = app_from_ui_review("p4b2");
}

fn setup_ui_review_p8s8(app: &mut App) {
    // P8S8 faderbank front (8 HP): 8 sliders, 8 LEDs, 8 switches. Rendering
    // caveat: the F-slider row renders only via M-family tokens — P tokens
    // don't cross the family→F cell boundary (review-relevant known state).
    *app = app_from_ui_review("p8s8");
}

fn setup_ui_review_db8e(app: &mut App) {
    // DB8E physical front (6 HP): eight buttons (B1.1..B1.8), one encoder
    // (E1.1), LEDs (L1.1..L1.8 shown; the panel carries 32). [db8e] is
    // registered in KNOWN_CONTROLLER_SECTIONS, so the tokens resolve to the
    // real db8e geometry (6 HP) instead of a fallback panel.
    *app = app_from_ui_review("db8e");
}

fn setup_ui_review_g8(app: &mut App) {
    // G8 physical front (4 HP): 8 gate jacks + 8 LEDs. g8.ini intentionally
    // lacks a [g8] section, so the fixture renders as the fallback
    // "Controller 1" panel — kept in the gallery to show the current
    // fallback state (review-relevant).
    *app = app_from_ui_review("g8");
}

fn setup_ui_review_x7(app: &mut App) {
    // X7 physical front (4 HP): 4 gate outputs, 8 LEDs, 1 USB switch. x7.ini
    // intentionally lacks an [x7] section, so the fixture renders as the
    // fallback "Controller 1" panel — kept in the gallery to show the
    // current fallback state (review-relevant).
    *app = app_from_ui_review("x7");
}

fn setup_ui_review_master18(app: &mut App) {
    // MASTER18 physical front (6 HP). The master faceplate resolves to
    // master18 whenever the patch addresses a CV jack above 8 (I9..I12 force
    // it here). Known caveat: master18.ini yields some invalid_jack Warnings
    // (validator caps at I1-I8) — acceptable, non-gating.
    *app = app_from_ui_review("master18");
}

fn setup_ui_review_all_uncovered(app: &mut App) {
    // One instance of every controller whose physical front is not yet
    // snapshot-covered (p4b2, p8s8, db8e, g8, x7, master18) in a single
    // patch — the side-by-side comparison row.
    *app = app_from_ui_review("all_uncovered");
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

fn setup_melody2(app: &mut App) {
    let patch =
        crate::patch::Patch::from_ini_file(Path::new("fixtures/droid_mpfs5melody2.ini")).unwrap();
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

fn setup_switch_value(app: &mut App) {
    *app = app_from_fixture("switch_value");
    for comp in &mut app.patch.as_mut().unwrap().hw_components {
        match comp.id.as_str() {
            "S1.1" => comp.state = ComponentState::Value(0.35),
            "S1.2" => comp.state = ComponentState::On,
            // P1.1 mid-way so the gallery row shows the fader track with its
            // lit amber rows (mirrors switch_value_app, task 1.2 design D1).
            "P1.1" => comp.state = ComponentState::Value(0.5),
            _ => {}
        }
    }
}

fn setup_paused_dim(app: &mut App) {
    *app = app_from_fixture("arpeggio1");
    app.processing_paused = true;
}

fn setup_disabled_circuit_graph(app: &mut App) {
    *app = app_from_fixture("cable_banner_combos");
    app.open_graph();
    app.disabled_circuits.insert((String::from("clocktool"), 0));
}
fn setup_physical_arpeggio_skeleton(app: &mut App) {
    *app = app_from_fixture("arpeggio1");
    app.physical_show_skeleton = true;
}

fn setup_physical_arpeggio_full(app: &mut App) {
    *app = app_from_fixture("arpeggio1");
    app.physical_show_skeleton = false;
}

fn setup_physical_multirow_rack_skeleton(app: &mut App) {
    *app = app_from_fixture("physical_multirow_rack");
    app.physical_show_skeleton = true;
}

fn setup_physical_multirow_rack_full(app: &mut App) {
    *app = app_from_fixture("physical_multirow_rack");
    app.physical_show_skeleton = false;
}

fn setup_optimizer_weighted(app: &mut App) {
    // Optimizer menu with the weight slider mid-range: `g o` opens the menu
    // (w = 0.0), four `]` presses step it to 0.4 through the real handler,
    // re-generating candidates under Weighted(0.4).
    *app = app_from_fixture("optimizer_latency");
    press(app, KeyCode::Char('g'));
    press(app, KeyCode::Char('o'));
    for _ in 0..4 {
        press(app, KeyCode::Char(']'));
    }
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
        id: "melody2_narrow_40",
        label: "droid_mpfs5melody2 · width 40 · boxed-cell fallback + ellipsis",
        width: 40,
        height: 30,
        setup: setup_melody2,
    },
    Scenario {
        id: "melody2_p2b8_uniform_60",
        label: "droid_mpfs5melody2 · width 60 · uniform P2B8 rows",
        width: 60,
        height: 150,
        setup: setup_melody2,
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
    Scenario {
        id: "switch_value_100",
        label: "switch_value · width 100 · switch renders token + Value %",
        width: 100,
        height: 30,
        setup: setup_switch_value,
    },
    Scenario {
        id: "paused_dim_100",
        label: "arpeggio1 · width 100 · processing paused (panels dim)",
        width: 100,
        height: 30,
        setup: setup_paused_dim,
    },
    Scenario {
        id: "disabled_circuit_graph_100",
        label: "cable_banner_combos · graph · clocktool disabled (dim node/edges)",
        width: 100,
        height: 40,
        setup: setup_disabled_circuit_graph,
    },
    Scenario {
        id: "optimizer_weight_100",
        label: "optimizer_latency · width 100 · g o menu · w = 0.4 (weighted obj per row)",
        width: 100,
        height: 30,
        setup: setup_optimizer_weighted,
    },
    // ── controller-front review matrix: fixtures/ui_review/* (6.2) ────────
    // Renders each newly-created controller-front fixture in the default
    // physical full view (rack faceplates) so the fronts can be reviewed
    // side-by-side with the DROID manual photos. Height 50 fits the full
    // 128.5 mm faceplate (≈39 rows at zoom 1.0) plus header/status chrome.
    // g8.ini/x7.ini intentionally lack their [g8]/[x7] sections and render
    // the fallback "Controller 1" panel — kept on purpose (fallback state is
    // review-relevant). p8s8's F-slider row renders only via M-family tokens
    // (P tokens don't cross family→F cell boundary); master18.ini yields
    // invalid_jack Warnings (validator caps at I1-I8) — both acceptable,
    // non-gating.
    Scenario {
        id: "ui_review_p4b2",
        label: "ui_review/p4b2 · width 100 · P4B2 front (5 HP, P1.1–P1.4 + B1.1/B1.2)",
        width: 100,
        height: 50,
        setup: setup_ui_review_p4b2,
    },
    Scenario {
        id: "ui_review_p8s8",
        label: "ui_review/p8s8 · width 100 · P8S8 faderbank front (8 HP) — F-sliders via M tokens",
        width: 100,
        height: 50,
        setup: setup_ui_review_p8s8,
    },
    Scenario {
        id: "ui_review_db8e",
        label: "ui_review/db8e · width 100 · DB8E front (6 HP, B1.1–B1.8 + E1.1)",
        width: 100,
        height: 50,
        setup: setup_ui_review_db8e,
    },
    Scenario {
        id: "ui_review_g8",
        label: "ui_review/g8 · width 100 · G8 front (4 HP) — no [g8] → fallback 'Controller 1'",
        width: 100,
        height: 50,
        setup: setup_ui_review_g8,
    },
    Scenario {
        id: "ui_review_x7",
        label: "ui_review/x7 · width 100 · X7 front (4 HP) — no [x7] → fallback 'Controller 1'",
        width: 100,
        height: 50,
        setup: setup_ui_review_x7,
    },
    Scenario {
        id: "ui_review_master18",
        label: "ui_review/master18 · width 100 · MASTER18 front (6 HP, I9+ forces) — invalid_jack warnings expected",
        width: 100,
        height: 50,
        setup: setup_ui_review_master18,
    },
    Scenario {
        id: "ui_review_all_uncovered",
        label: "ui_review/all_uncovered · width 100 · all six controller fronts side-by-side",
        width: 100,
        height: 50,
        setup: setup_ui_review_all_uncovered,
    },
    // ── physical-view matrix (task 5.2): the rack in skeleton | full ───────
    // Each fixture renders both presentations at the same viewport so the
    // matrix itself is the D5 coincidence proof (full rect == skeleton rect);
    // `physical_gallery_pairs_full_skeleton_coincide` asserts that contract
    // per pair. Skeleton shows module outlines + `·` cells; full shows the
    // 1:1 control geometry (ui-hw-alignment).
    Scenario {
        id: "physical_arpeggio_skeleton_80",
        label: "physical rack arpeggio1 · width 80 · skeleton",
        width: 80,
        height: 30,
        setup: setup_physical_arpeggio_skeleton,
    },
    Scenario {
        id: "physical_arpeggio_full_80",
        label: "physical rack arpeggio1 · width 80 · full",
        width: 80,
        height: 30,
        setup: setup_physical_arpeggio_full,
    },
    Scenario {
        id: "physical_arpeggio_skeleton_120",
        label: "physical rack arpeggio1 · width 120 · skeleton",
        width: 120,
        height: 30,
        setup: setup_physical_arpeggio_skeleton,
    },
    Scenario {
        id: "physical_arpeggio_full_120",
        label: "physical rack arpeggio1 · width 120 · full",
        width: 120,
        height: 30,
        setup: setup_physical_arpeggio_full,
    },
    Scenario {
        id: "physical_multirow_rack_skeleton_80",
        label: "physical rack multirow · width 80 · skeleton (2 rows + fold bar)",
        width: 80,
        height: 70,
        setup: setup_physical_multirow_rack_skeleton,
    },
    Scenario {
        id: "physical_multirow_rack_full_80",
        label: "physical rack multirow · width 80 · full (2 rows + fold bar)",
        width: 80,
        height: 70,
        setup: setup_physical_multirow_rack_full,
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
        // Render-outlier flags (task 3.2) are collected per theme first so the
        // scenario cell can carry an aggregate marker as well.
        let mut cells: Vec<(&str, &str, bool, String)> = Vec::with_capacity(theme::THEMES.len());
        let mut any_degraded = false;
        for &theme_name in theme::THEMES {
            let _guard = ThemedGuard::pin(theme_name);
            let mut app = App::new();
            (scenario.setup)(&mut app);
            let buf = buffer_for(&mut app, scenario.width, scenario.height);
            let mut ansi = buffer_to_ansi(&buf);
            let html = buffer_to_html(&buf);

            // Score the scenario's patch at the rendered width under the pinned
            // theme, exactly like ui.rs `render_outlier_hint`. `Err` (schema
            // drift, design D1) is treated as "not flagged".
            let degraded = match app.patch.as_ref().and_then(|patch| {
                score_render(&RenderFeatures::extract(
                    patch,
                    scenario.width,
                    theme::active(),
                ))
                .ok()
                .flatten()
            }) {
                Some(outlier) => {
                    any_degraded = true;
                    ansi = format!(
                        "⚠ degraded ({:?}) — use ≥ {} cols\n{ansi}",
                        outlier.channel, outlier.recommended_width
                    );
                    true
                }
                None => false,
            };

            // Write per-scenario ANSI for inspectability (ephemeral, gitignored)
            let ansi_path = out_dir.join(format!("{}_{}.ansi", scenario.id, theme_name));
            fs::write(&ansi_path, &ansi)?;

            // Also write raw HTML cell for debugging if needed
            let html_cell_path = out_dir.join(format!("{}_{}.html", scenario.id, theme_name));
            fs::write(&html_cell_path, &html)?;

            cells.push((theme_name, scenario.id, degraded, html));
        }

        rows_html.push_str(&format!(
            "    <tr><td class=\"scenario\">{}{}</td>",
            html_escape(scenario.label),
            if any_degraded {
                " <span class=\"degraded\">⚠ degraded</span>"
            } else {
                ""
            }
        ));
        for (theme_name, scenario_id, degraded, html) in cells {
            let marker = if degraded {
                "<span class=\"degraded\">⚠ degraded</span>\n"
            } else {
                ""
            };
            let attr = if degraded {
                " data-degraded=\"true\""
            } else {
                ""
            };
            rows_html.push_str(&format!(
                "<td data-theme=\"{theme_name}\" data-scenario=\"{scenario_id}\"{attr}>{marker}<pre class=\"cell\">{html}</pre></td>"
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
.degraded {{ color: #ffd500; font-weight: bold; font-size: 0.75rem; display: block; margin: 0 0 2px; }}
td[data-degraded="true"] {{ border-left: 3px solid #ffd500; }}
</style>
</head>
<body>
<h1>droid_tui — visual gallery (TestBackend → ANSI → HTML)</h1>
<p class="legend">One row per scenario, columns <code>classic</code> / <code>terminal</code> / <code>mono</code> (widths 80/120, viewer open/closed, shift active). Each cell is the same HTML from <code>buffer_to_html</code> used in insta snapshots. Generated via <code>cargo run --bin snapshot-gallery</code> or <code>GENERATE_GALLERY=1 cargo test</code>. Cells marked <code>⚠ degraded</code> are scenarios the render-outlier scorer predicts degraded at that width/theme.</p>
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
