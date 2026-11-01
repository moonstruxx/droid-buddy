//! Headless end-to-end smoke test (`droid_tui --self-test [patch.ini]`).
//!
//! CI never opens a window and rarely has a display, so the class of defect that
//! only shows up when the real paint dispatch runs — a pane that publishes no
//! hit geometry, a dropped input report, a camera that is never seeded — reached
//! `main` and was found by hand (change `module-ui-labels`). This module drives
//! the real [`App`] through the same per-frame dispatch the window loop runs
//! (`paint_panes` + `paint_overlays`) inside a plain [`egui::Context`], then
//! asserts the geometry the input layer depends on was published and that the
//! published geometry round-trips through the window input path.
//!
//! No GPU, no window, no MIDI: every step is CPU-only, so the same run works on
//! a headless CI runner of any OS. `run` never panics — a failed check is data,
//! and the caller decides the exit code.

use std::path::Path;

use crate::app::App;

/// The band the self-test paints into (egui points).
const WINDOW_W: f32 = 1280.0;
const WINDOW_H: f32 = 800.0;

/// One named check and its outcome.
pub struct Check {
    /// Stable check name, printed by the CLI.
    pub name: &'static str,
    /// `Ok(())` passed, `Err` names what the run observed instead.
    pub outcome: Result<(), String>,
}

/// The outcome of a self-test run: every check, in the order it ran.
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    /// True when every check passed.
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.outcome.is_ok())
    }

    /// One line per check — `ok   <name>` or `FAIL <name>: <reason>` — plus a
    /// closing `N/M checks passed` line.
    pub fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .checks
            .iter()
            .map(|check| match &check.outcome {
                Ok(()) => format!("ok   {}", check.name),
                Err(reason) => format!("FAIL {}: {reason}", check.name),
            })
            .collect();
        let passed = self.checks.iter().filter(|c| c.outcome.is_ok()).count();
        lines.push(format!("{passed}/{} checks passed", self.checks.len()));
        lines
    }

    /// The failing checks, as `name: reason`, for an error message or a log.
    pub fn failures(&self) -> Vec<String> {
        self.checks
            .iter()
            .filter_map(|check| match &check.outcome {
                Ok(()) => None,
                Err(reason) => Some(format!("{}: {reason}", check.name)),
            })
            .collect()
    }
}

/// Run the smoke test: load the patch (or the bundled demo), then drive one
/// startup frame, the module-UI click round-trip, the skeleton presentation, the
/// graph pane and the help overlay through the headless paint dispatch.
pub fn run(patch: Option<&Path>) -> Report {
    let mut checks = Vec::new();
    let mut app = App::new();

    checks.push(Check {
        name: "patch loads",
        outcome: load_patch(&mut app, patch),
    });

    // Frame 1: the startup arrangement (module UI in the left big pane, source
    // viewer in the first small pane) must paint both titles and publish the
    // module UI's cell hit rects.
    let startup = paint(&mut app);
    checks.push(Check {
        name: "startup band paints",
        outcome: check_startup(&app, &startup),
    });
    drop(startup);

    // The published rects are the input layer's only knowledge of the layout, so
    // a click on one of them must select that element.
    checks.push(Check {
        name: "published geometry drives input",
        outcome: check_click(&mut app),
    });

    // The skeleton presentation must draw from the same cells: the hit geometry
    // it republishes has to be identical to the full render's (spec 5.1).
    checks.push(Check {
        name: "skeleton keeps hit geometry",
        outcome: check_skeleton(&mut app),
    });

    // The graph pane: scene build, camera seeding, canvas size. The graph goes
    // to the left big pane and replaces the module UI there (it is a Big-class
    // view), so this runs after the module-UI checks.
    app.open_graph();
    // Two frames: the first seeds the camera against the published canvas, the
    // second paints through it.
    let _ = paint(&mut app);
    let graph_frame = paint(&mut app);
    let graph_labels = text_labels(&graph_frame);
    checks.push(Check {
        name: "graph pane paints",
        outcome: check_graph(&app, &graph_labels),
    });

    // A centered overlay must add shapes on top of the band it covers. The
    // baseline is taken immediately before opening it: earlier checks change the
    // band's own label count (a selection draws a hover callout), so comparing
    // against the startup frame would compare across different states.
    let baseline = text_labels(&paint(&mut app)).len();
    app.open_help();
    let help = paint(&mut app);
    let help_labels = text_labels(&help).len();
    app.close_help();
    checks.push(Check {
        name: "help overlay paints",
        outcome: if help_labels > baseline {
            Ok(())
        } else {
            Err(format!(
                "{help_labels} labels with the help modal open, {baseline} without"
            ))
        },
    });
    drop(help);

    Report { checks }
}

/// Load `patch`, or the bundled demo when none is given, and reject the run when
/// the load was refused (validation `Error` findings gate a load).
fn load_patch(app: &mut App, patch: Option<&Path>) -> Result<(), String> {
    if let Some(path) = patch.filter(|path| !path.as_os_str().is_empty()) {
        let parsed = crate::patch::Patch::from_ini_file(path)
            .map_err(|err| format!("{}: {err}", path.display()))?;
        if !app.load_patch_at(path, parsed) {
            return Err(format!("{} was rejected by validation", path.display()));
        }
        return Ok(());
    }
    let parsed = crate::patch::Patch::from_ini_str(crate::patch::DEMO_INI, String::from("demo"))
        .map_err(|err| format!("bundled demo patch: {err}"))?;
    if !app.load_patch(parsed) {
        return Err("bundled demo patch was rejected by validation".to_string());
    }
    Ok(())
}

/// One headless frame through the window loop's dispatch: the pane band plus the
/// centered overlays, exactly as `main.rs` runs them.
fn paint(app: &mut App) -> egui::FullOutput {
    let ctx = egui::Context::default();
    let win = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WINDOW_W, WINDOW_H));
    let raw = egui::RawInput {
        screen_rect: Some(win),
        ..Default::default()
    };
    let scene = super::graph::build_scene_spec(
        app,
        crate::theme::active(),
        super::graph::graph_pane_rect(app, win).unwrap_or(win),
    );
    let mut out = ctx.run_ui(raw, |ui| {
        super::paint_panes(app, ui, scene.as_ref(), &[]);
        super::paint_overlays(app, ui);
    });
    // epaint aborts the process when a `FullOutput` with unapplied deltas drops
    // mid-report, which would turn one failed check into a lost run.
    out.textures_delta.clear();
    out
}

fn text_labels(out: &egui::FullOutput) -> Vec<String> {
    out.shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Text(text) => Some(text.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

/// The startup frame: every startup pane published its cell, both pane titles
/// painted, and the module UI published element hit rects.
fn check_startup(app: &App, out: &egui::FullOutput) -> Result<(), String> {
    if app.pane_hit_rects.len() < 2 {
        return Err(format!(
            "{} panes published, expected the module UI and the source viewer",
            app.pane_hit_rects.len()
        ));
    }
    let labels = text_labels(out);
    for want in ["Module UI", "Source"] {
        if !labels.iter().any(|label| label.contains(want)) {
            return Err(format!(
                "no {want} pane title among {} painted labels",
                labels.len()
            ));
        }
    }
    let cells = app.component_rects.len();
    if cells == 0 {
        return Err("the module UI published no component hit rects".to_string());
    }
    // A published rect must be a real rect inside the band, not a zero-size relic.
    let (_, rect) = app.component_rects[0];
    if rect.width == 0 || rect.height == 0 {
        return Err(format!(
            "first component hit rect is degenerate: {}x{}",
            rect.width, rect.height
        ));
    }
    Ok(())
}

/// A click on a published component rect, mapped through the window input path,
/// must select that element.
fn check_click(app: &mut App) -> Result<(), String> {
    let Some(&(index, rect)) = app.component_rects.first() else {
        return Err("no component rects to click".to_string());
    };
    let token = app
        .patch
        .as_ref()
        .and_then(|patch| patch.hw_components.get(index))
        .map(|component| component.id.clone())
        .ok_or_else(|| format!("published rect {index} names no hardware component"))?;
    let center = (
        f32::from(rect.x) + f32::from(rect.width) / 2.0,
        f32::from(rect.y) + f32::from(rect.height) / 2.0,
    );
    let frame = super::WindowFrame {
        pointer: Some(center),
        primary_pressed: true,
        ..Default::default()
    };
    crate::handler::handle_graph_window_frame(&frame, app);
    match app.selected_component.as_deref() {
        Some(selected) if selected == token => Ok(()),
        other => Err(format!(
            "click at ({:.1}, {:.1}) over {token} selected {other:?}",
            center.0, center.1
        )),
    }
}

/// The skeleton presentation republishes the same hit geometry as the full
/// render (module-ui §5.1), so no cell may move or vanish when `s` toggles it.
fn check_skeleton(app: &mut App) -> Result<(), String> {
    let full = app.component_rects.clone();
    app.physical_show_skeleton = true;
    let _ = paint(app);
    let skeleton = app.component_rects.clone();
    app.physical_show_skeleton = false;
    let _ = paint(app);
    if full != skeleton {
        let moved = full.iter().zip(&skeleton).filter(|(a, b)| a != b).count();
        return Err(format!(
            "{} hit rects in the full render, {} in the skeleton, {moved} differ",
            full.len(),
            skeleton.len()
        ));
    }
    Ok(())
}

/// The graph pane must publish its canvas size, seed the camera, and project
/// every graph node into a finite, non-degenerate spec rect — the inputs `c`,
/// `Shift+c`, wheel zoom, arrow pan and node dragging all read. Node hit-testing
/// itself is world-space (ADR 40), so it is exercised by the scene, not by a
/// published pixel rect.
fn check_graph(app: &App, painted: &[String]) -> Result<(), String> {
    let nodes = app.graph.as_ref().map_or(0, |graph| graph.nodes.len());
    if nodes == 0 {
        return Err("the graph model was never built".to_string());
    }
    let Some((canvas_w, canvas_h)) = app.graph_canvas_px else {
        return Err("the graph pane published no canvas size".to_string());
    };
    if app.graph_camera.is_none() {
        return Err("the graph camera was never seeded".to_string());
    }
    let pane = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(canvas_w.max(1.0), canvas_h.max(1.0)),
    );
    let Some(scene) = super::graph::build_scene_spec(app, crate::theme::active(), pane) else {
        return Err("the graph pane built no scene".to_string());
    };
    if scene.nodes.len() != nodes {
        return Err(format!(
            "scene has {} nodes for a {nodes}-node graph",
            scene.nodes.len()
        ));
    }
    if let Some(node) = scene.nodes.iter().find(|node| {
        !(node.x.is_finite()
            && node.y.is_finite()
            && node.w.is_finite()
            && node.w > 0.0
            && node.h.is_finite()
            && node.h > 0.0)
    }) {
        return Err(format!(
            "node {:?} projects to a degenerate rect ({}, {} {}x{})",
            node.circuit, node.x, node.y, node.w, node.h
        ));
    }
    if painted.is_empty() {
        return Err("the graph frame painted no text".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smoke test itself, on the bundled demo patch — the guard that keeps
    /// this module honest without a CI runner.
    #[test]
    fn self_test_passes_on_the_demo_patch() {
        let report = run(None);
        assert!(report.passed(), "{:?}", report.lines());
        assert_eq!(report.checks.len(), 6, "every check ran");
    }
}
