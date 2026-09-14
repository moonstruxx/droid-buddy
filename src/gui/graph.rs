//! Graph canvas painting and interaction for the egui window surface (task
//! 1.2 split: the canvas moved out of the window shell).
//!
//! Everything here either draws from the shared [`SceneSpec`] produced by the
//! `GraphCamera` (one spec pixel = one egui point) or computes pure geometry
//! over it (hit-testing, marquee, minimap, tooltip), so the module tests
//! headless under `cargo test`. [`paint_scene`] is the draw routine the shell
//! dispatches to each frame; the camera helpers (`camera_pan`,
//! `camera_zoom_about`) are the window's bridge to `App::graph_camera`.

use std::collections::HashMap;

use super::{MarqueeSelection, WindowFrame};
use crate::app::{App, GRAPH_WINDOW_NODE_H, GRAPH_WINDOW_NODE_W};
use crate::graph::NodeKind;
use crate::graph_render::{
    CableKind, ClusterSpec, EdgeDiffState, EdgeSpec, GraphCamera, NodeSpec, SceneSpec, WorldBounds,
};
use crate::theme::Theme;

/// Clamps a normalized ramp axis position into 0..1.
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Draw one scene frame into the window canvas (design D3/D5): opaque
/// background, cluster containers, then cables with direction arrows, then
/// node frames with ports and titles. The spec is pixel-space output of the
/// shared `GraphCamera`, painted 1:1 (one spec pixel = one egui point), so
/// both surfaces show the same view. Colors come only from the resolved spec
/// RGB — never theme tokens below the spec.
pub(crate) fn paint_scene(
    painter: &egui::Painter,
    canvas: egui::Vec2,
    scene: Option<&SceneSpec>,
    ctx: &egui::Context,
    selected: &[usize],
) {
    let Some(spec) = scene else {
        return; // No scene: the swapchain clear color shows through.
    };
    painter.rect_filled(
        egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
        0.0,
        rgb(spec.background),
    );

    for cluster in &spec.clusters {
        let rect = egui::Rect::from_min_size(
            egui::pos2(cluster.x, cluster.y),
            egui::vec2(cluster.w, cluster.h),
        );
        painter.rect(
            rect,
            egui::CornerRadius::same(2),
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, rgb(cluster.border)),
            egui::StrokeKind::Inside,
        );
        if !cluster.title.is_empty() {
            painter.text(
                rect.left_top() + egui::vec2(6.0, 3.0),
                egui::Align2::LEFT_TOP,
                &cluster.title,
                egui::FontId::proportional(12.0),
                rgb(cluster.title_color),
            );
        }
    }

    for edge in &spec.edges {
        // A quadratic Bézier is exactly a cubic with control points at
        // (2C+S)/3 and (2C+E)/3; egui paints cubic strokes.
        let c1 = egui::pos2(
            (2.0 * edge.ctrl.0 + edge.start.0) / 3.0,
            (2.0 * edge.ctrl.1 + edge.start.1) / 3.0,
        );
        let c2 = egui::pos2(
            (2.0 * edge.ctrl.0 + edge.end.0) / 3.0,
            (2.0 * edge.ctrl.1 + edge.end.1) / 3.0,
        );
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            [
                egui::pos2(edge.start.0, edge.start.1),
                c1,
                c2,
                egui::pos2(edge.end.0, edge.end.1),
            ],
            false,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(edge.width, rgb(edge.color)),
        ));
        paint_arrow(painter, edge);
    }

    for node in &spec.nodes {
        let rect =
            egui::Rect::from_min_size(egui::pos2(node.x, node.y), egui::vec2(node.w, node.h));
        let radius = node.radius.clamp(0.0, node.w.min(node.h) / 2.0);
        let corner = egui::CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: radius as u8,
            se: radius as u8,
        };
        painter.rect_filled(rect, corner, rgb(node.fill));
        if node.border_width > 0.0 {
            painter.rect(
                rect,
                corner,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(node.border_width, rgb(node.border)),
                egui::StrokeKind::Middle,
            );
        }
        // Port markers on the left (input) / right (output) edge midpoint,
        // like the terminal tile's ◉ / ●; drawn in the frame's border color.
        let port_r = (node.h * 0.16).clamp(3.0, 8.0);
        let mid_y = node.y + node.h / 2.0;
        if node.input_port {
            painter.circle_filled(egui::pos2(node.x, mid_y), port_r, rgb(node.border));
        }
        if node.output_port {
            painter.circle_filled(egui::pos2(node.x + node.w, mid_y), port_r, rgb(node.border));
        }
        if !node.label.is_empty() {
            let px = (node.h * 0.42).clamp(8.0, 40.0);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &node.label,
                egui::FontId::monospace(px),
                rgb(node.label_color),
            );
        }
    }

    // Task 3.2/3.3: selection, navigation and inspection overlays.
    paint_polish(painter, canvas, scene, ctx, selected);
}

/// Fill the direction arrow at an edge's `end`, following the scene spec's
/// curve: tangent `B'(1) = 2·(end − ctrl)`, triangle sized from the width.
fn paint_arrow(painter: &egui::Painter, edge: &EdgeSpec) {
    let (tx, ty) = (
        2.0 * (edge.end.0 - edge.ctrl.0),
        2.0 * (edge.end.1 - edge.ctrl.1),
    );
    let len = (tx * tx + ty * ty).sqrt();
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (tx / len, ty / len);
    let (nx, ny) = (-uy, ux);
    let arrow_len = (8.0 + edge.width * 2.0).min(16.0);
    let half = (4.0 + edge.width).min(8.0);
    let base = egui::pos2(edge.end.0 - ux * arrow_len, edge.end.1 - uy * arrow_len);
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(edge.end.0, edge.end.1),
            base + egui::vec2(nx * half, ny * half),
            base - egui::vec2(nx * half, ny * half),
        ],
        rgb(edge.color),
        egui::Stroke::NONE,
    ));
}

fn rgb((r, g, b): (u8, u8, u8)) -> egui::Color32 {
    // The scene spec carries theme tokens already resolved to RGB triples;
    // this is the shared unpack the bridge documents (theme.rs:egui_from_rgb)
    // so the graph canvas and the kitty path stay on the same single hop.
    crate::theme::active().egui_from_rgb((r, g, b))
}

fn rgba((r, g, b): (u8, u8, u8), a: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Wheel-zoom sensitivity: exponent multiplier on the scroll delta so a
/// typical wheel tick reads as a gentle zoom step.
pub(crate) const ZOOM_SENSITIVITY: f32 = 0.01;
/// Largest factor a single scroll event may apply; bounds wheel zoom so a
/// fast spin cannot blow the camera off the scene.
pub(crate) const MAX_ZOOM_STEP: f32 = 1.5;
/// Fixed minimap size `(w, h)` in egui points, bottom-left corner.
const MINIMAP_SIZE: (f32, f32) = (180.0, 120.0);
/// Margin between the minimap panel and the canvas edge.
const MINIMAP_PAD: f32 = 10.0;
/// Pixel margin around a node frame when attributing an edge endpoint to it.
const EDGE_ATTRIB_MARGIN: f32 = 8.0;
/// Minimum drag extent before a marquee is reported; a click on empty canvas
/// (no movement) is not a marquee.
const MARQUEE_MIN_DRAG: f32 = 2.0;

/// Copy of `cam` panned by `(dx, dy)` spec pixels. Exposed so the windowed
/// loop can apply the window's middle-drag pan to the shared
/// `App::graph_camera` through the existing `GraphCamera::pan_by` API.
/// Pure and window-free.
pub fn camera_pan(cam: &GraphCamera, dx: f32, dy: f32) -> GraphCamera {
    let mut next = *cam;
    next.pan_by(dx, dy);
    next
}

/// Copy of `cam` zoomed by `factor` about the spec-pixel anchor `(ax, ay)`:
/// the world point under the anchor stays under the anchor after the zoom.
/// Mirrors the terminal `+`/`-` zoom, but anchored at the cursor instead of
/// the canvas centre. Pure and window-free.
pub fn camera_zoom_about(cam: &GraphCamera, factor: f32, anchor_px: (f32, f32)) -> GraphCamera {
    let mut next = *cam;
    let (wx, wy) = next.pixel_to_world(anchor_px.0, anchor_px.1);
    next.zoom_by(factor, (wx, wy));
    next
}
/// Availability the window fit frames the world into: the fixed design
/// viewport (the loop owns the window; the old kitty renderer used the same
/// 1280×800 canvas) minus one node frame so edge nodes never clip.
const WINDOW_FIT_VIEWPORT_W: f32 = 1280.0;
const WINDOW_FIT_VIEWPORT_H: f32 = 800.0;

/// First-frame camera fit for the desktop graph window, seeded into
/// `App::graph_camera` when it is still unset so the scene frames the whole
/// graph instead of painting the layered seed plane off-viewport (bug: the
/// window opened black). Reused afterwards through the `camera_pan` /
/// `camera_zoom_about` mappings, so user pan/zoom survives. Dependency-filter
/// aware via the caller's positions slice. Pure and window-free.
pub fn graph_window_fit_camera(positions: &[(f32, f32)]) -> GraphCamera {
    let node_w = crate::app::GRAPH_WINDOW_NODE_W;
    let node_h = crate::app::GRAPH_WINDOW_NODE_H;
    let avail_w = (WINDOW_FIT_VIEWPORT_W - node_w).max(1.0);
    let avail_h = (WINDOW_FIT_VIEWPORT_H - node_h).max(1.0);
    GraphCamera::fit_to_world(
        WorldBounds::from_positions(positions),
        (avail_w, avail_h),
        WINDOW_FIT_MIN_NODE_PX,
    )
}

/// Fit-zoom floor, preserved from the old window builder's
/// `GRAPH_NODE_WIDTH as f32 * GRAPH_CELL_W_PX / 80.0` (22 × 8 / 80): a tiny
/// ceiling that only keeps the fit from collapsing zoom below legibility on
/// very large graphs.
const WINDOW_FIT_MIN_NODE_PX: f32 = 2.2;

/// Index of the `scene` node whose pixel frame contains `(px, py)`, first
/// match wins (mirrors the terminal handler's hit-testing over the spec's own
/// pixel rects). `None` over empty canvas.
fn node_at(scene: &SceneSpec, px: f32, py: f32) -> Option<usize> {
    scene
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| px >= n.x && px < n.x + n.w && py >= n.y && py < n.y + n.h)
        .map(|(i, _)| i)
}

/// The axis-aligned rect spanning two pixel corners, normalized so the drag
/// direction is irrelevant.
fn normalize_rect(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    (
        a.0.min(b.0),
        a.1.min(b.1),
        (a.0 - b.0).abs(),
        (a.1 - b.1).abs(),
    )
}

/// Indices of `scene` nodes whose pixel frames intersect `rect`, strict AABB
/// overlap (a zero-area touch on a shared edge does not count).
fn nodes_in_rect(scene: &SceneSpec, (x, y, w, h): (f32, f32, f32, f32)) -> Vec<usize> {
    scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.x < x + w && n.x + n.w > x && n.y < y + h && n.y + n.h > y)
        .map(|(i, _)| i)
        .collect()
}

/// The active marquee over `scene` for the given pointer state: present only
/// while the primary button is held and the press began on empty canvas (a
/// press on a node is a node drag, not a marquee). `None` when idle.
pub(super) fn frame_marquee(
    scene: Option<&SceneSpec>,
    i: &egui::InputState,
) -> Option<MarqueeSelection> {
    if !i.pointer.primary_down() {
        return None;
    }
    let origin = i.pointer.press_origin()?;
    let scene = scene?;
    if node_at(scene, origin.x, origin.y).is_some() {
        return None;
    }
    let cur = i.pointer.latest_pos()?;
    let rect = normalize_rect((origin.x, origin.y), (cur.x, cur.y));
    if rect.2 < MARQUEE_MIN_DRAG && rect.3 < MARQUEE_MIN_DRAG {
        return None; // a click, not a marquee drag
    }
    let nodes = nodes_in_rect(scene, rect);
    Some(MarqueeSelection { rect, nodes })
}

/// The window-local selection after this frame: a live or committed marquee
/// replaces the previous selection (so the highlight follows the drag), a
/// fresh press that is not a marquee (a node grab or a plain empty click)
/// clears it, and anything else keeps it. Pure so the marquee lifecycle tests
/// without a window.
pub(super) fn next_selection(frame: &WindowFrame, previous: &[usize]) -> Vec<usize> {
    if let Some(marquee) = &frame.marquee {
        return marquee.nodes.clone();
    }
    if frame.primary_pressed {
        return Vec::new();
    }
    previous.to_vec()
}

/// Scene pixel bounds `(min_x, min_y, max_x, max_y)` over every node frame,
/// for the minimap's world-to-mini mapping. All zeros on an empty scene.
fn scene_bounds(scene: &SceneSpec) -> (f32, f32, f32, f32) {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for n in &scene.nodes {
        min_x = min_x.min(n.x);
        min_y = min_y.min(n.y);
        max_x = max_x.max(n.x + n.w);
        max_y = max_y.max(n.y + n.h);
    }
    if scene.nodes.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}

/// Minimap layout: the bottom-left panel rect, the scaled viewport box (the
/// whole canvas in scene space), and each node's scaled frame. Pure geometry
/// so it tests without a window.
struct Minimap {
    panel: (f32, f32, f32, f32),
    viewport: (f32, f32, f32, f32),
    nodes: Vec<(f32, f32, f32, f32)>,
}

fn minimap_layout(scene: &SceneSpec, canvas: egui::Vec2) -> Option<Minimap> {
    if scene.nodes.is_empty() {
        return None;
    }
    let (mw, mh) = MINIMAP_SIZE;
    let (px, py) = (MINIMAP_PAD, canvas.y - mh - MINIMAP_PAD);
    let panel = (px, py, mw, mh);
    let (bx, by, bw, bh) = scene_bounds(scene);
    if bw <= 0.0 || bh <= 0.0 {
        return None;
    }
    // A layout that fits the canvas needs no map; only show the minimap when
    // the world overflows the viewport in at least one axis.
    if bw <= canvas.x && bh <= canvas.y {
        return None;
    }
    let inner = (px + 4.0, py + 4.0, mw - 8.0, mh - 8.0);
    let (ix, iy, iw, ih) = inner;
    let sx = iw / bw;
    let sy = ih / bh;
    let map = |wx: f32, wy: f32| (ix + (wx - bx) * sx, iy + (wy - by) * sy);
    let nodes = scene
        .nodes
        .iter()
        .map(|n| {
            let (x0, y0) = map(n.x, n.y);
            let (x1, y1) = map(n.x + n.w, n.y + n.h);
            (x0, y0, x1 - x0, y1 - y0)
        })
        .collect();
    // The visible canvas occupies `[0,0] x canvas` in scene-pixel space.
    let (v0, v0y) = map(0.0, 0.0);
    let (v1, v1y) = map(canvas.x, canvas.y);
    // Clamp the viewport box to the panel: when the canvas is larger than the
    // scene (zoomed out so the whole scene fits), the full-canvas box would
    // overrun the panel; clamping makes it read as "the whole scene is
    // visible". When zoomed in, the box is already a sub-rect and is
    // unchanged.
    let (px2, py2, pw2, ph2) = panel;
    let cx = v0.clamp(px2, px2 + pw2);
    let cy = v0y.clamp(py2, py2 + ph2);
    let cx2 = (v1).clamp(px2, px2 + pw2);
    let cy2 = (v1y).clamp(py2, py2 + ph2);
    let viewport = (cx, cy, cx2 - cx, cy2 - cy);
    Some(Minimap {
        panel,
        viewport,
        nodes,
    })
}

/// The node whose frame is nearest to a scene point, within a margin: used to
/// attribute an edge's start/end port to its source/sink node for the latency
/// readout. The nearest-centre tiebreak keeps two abutting nodes from both
/// claiming a shared-edge port.
fn nearest_node_at(scene: &SceneSpec, px: f32, py: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, n) in scene.nodes.iter().enumerate() {
        if px >= n.x - EDGE_ATTRIB_MARGIN
            && px <= n.x + n.w + EDGE_ATTRIB_MARGIN
            && py >= n.y - EDGE_ATTRIB_MARGIN
            && py <= n.y + n.h + EDGE_ATTRIB_MARGIN
        {
            let dx = n.x + n.w / 2.0 - px;
            let dy = n.y + n.h / 2.0 - py;
            let d = dx * dx + dy * dy;
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
    }
    best.map(|(i, _)| i)
}

/// Tooltip text for a hovered node: the circuit label (with the occurrence
/// index when the name repeats) plus a latency readout aggregated from the
/// node's outgoing edges' resolved [`EdgeLatency`] states. No latency data
/// yields just the label. The window cannot consult `latency::CostModel` (the
/// scene is the only shared source it sees), so it reports the same ramp
/// classification the terminal colours edges with.
fn node_tooltip(scene: &SceneSpec, node_index: usize) -> String {
    let node = &scene.nodes[node_index];
    let repeated = scene
        .nodes
        .iter()
        .filter(|n| n.circuit == node.circuit && !node.circuit.is_empty())
        .count()
        > 1;
    let mut label = if node.circuit.is_empty() {
        format!("node #{node_index}")
    } else if repeated {
        format!("{} ({})", node.circuit, node.instance_index)
    } else {
        node.circuit.clone()
    };
    let outgoing = scene
        .edges
        .iter()
        .filter(|e| nearest_node_at(scene, e.start.0, e.start.1) == Some(node_index))
        .collect::<Vec<_>>();
    let latencies = outgoing
        .iter()
        .filter_map(|e| e.latency)
        .collect::<Vec<_>>();
    if latencies.is_empty() {
        return label;
    }
    let hot = latencies.iter().map(|l| l.ramp_stop).max().unwrap_or(0);
    let back = latencies.iter().filter(|l| l.back_edge).count();
    label.push_str(&format!(
        " · latency {hot}/4 · {back} back edge{}",
        if back == 1 { "" } else { "s" }
    ));
    label
}

/// Draws the canvas polish overlays on top of `paint_scene`: the minimap, the
/// active marquee selection, the committed selection's node highlight, and the
/// hovered node's latency tooltip. Every color derives from the resolved scene
/// spec RGB (never hardcoded). The tooltip is drawn last so it stays readable
/// above marquee, selection, and minimap.
fn paint_polish(
    painter: &egui::Painter,
    canvas: egui::Vec2,
    scene: Option<&SceneSpec>,
    ctx: &egui::Context,
    selected: &[usize],
) {
    let Some(spec) = scene else {
        return;
    };
    let accent = spec
        .nodes
        .first()
        .map(|n| n.border)
        .or_else(|| spec.clusters.first().map(|c| c.border))
        .unwrap_or(spec.background);
    if let Some(minimap) = minimap_layout(spec, canvas) {
        paint_minimap(painter, spec, &minimap, accent);
    }
    // The committed marquee selection: an accent overlay border on each picked
    // node frame, drawn above the scene but below the in-progress marquee rect
    // and the tooltip. Out-of-range indices (stale after a rebuild) are skipped.
    for &i in selected {
        let Some(node) = spec.nodes.get(i) else {
            continue;
        };
        let rect =
            egui::Rect::from_min_size(egui::pos2(node.x, node.y), egui::vec2(node.w, node.h));
        painter.rect(
            rect,
            egui::CornerRadius::same(node.radius.clamp(0.0, node.w.min(node.h) / 2.0) as u8),
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(2.0, rgb(accent)),
            egui::StrokeKind::Middle,
        );
    }
    // Read the input state under the context lock, then paint both overlays
    // after it: text layout takes the context write lock again, so it must not
    // run inside the ctx.input closure (same-thread reentrant write deadlock).
    let (marquee, hover) = ctx.input(|i| {
        let marquee = frame_marquee(Some(spec), i);
        let hover = i
            .pointer
            .latest_pos()
            .and_then(|pos| node_at(spec, pos.x, pos.y).map(|idx| (pos, idx)));
        (marquee, hover)
    });
    if let Some(marquee) = marquee {
        let (x, y, w, h) = marquee.rect;
        let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
        painter.rect_filled(rect, 0.0, rgba(accent, 30));
        painter.rect(
            rect,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, rgb(accent)),
            egui::StrokeKind::Inside,
        );
    }
    if let Some((pos, idx)) = hover {
        paint_tooltip(painter, spec, canvas, pos, idx);
    }
}

/// The hovered node's tooltip card: a translucent backdrop with the accent
/// border, placed above-right of the cursor and clamped to the canvas.
fn paint_tooltip(
    painter: &egui::Painter,
    spec: &SceneSpec,
    canvas: egui::Vec2,
    pos: egui::Pos2,
    node_index: usize,
) {
    let node = &spec.nodes[node_index];
    let font = egui::FontId::proportional(13.0);
    let galley =
        painter.layout_no_wrap(node_tooltip(spec, node_index), font, rgb(node.label_color));
    let pad = 6.0;
    let size = galley.size() + egui::vec2(pad * 2.0, pad * 2.0);
    let min_x = (pos.x + 12.0).clamp(0.0, (canvas.x - size.x).max(0.0));
    let min_y = (pos.y - size.y - 8.0).clamp(0.0, (canvas.y - size.y).max(0.0));
    let min = egui::pos2(min_x, min_y);
    let rect = egui::Rect::from_min_size(min, size);
    painter.rect(
        rect,
        egui::CornerRadius::same(4),
        rgba(spec.background, 235),
        egui::Stroke::new(1.0, rgb(node.border)),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        rect.min + egui::vec2(pad, pad),
        galley,
        rgb(node.label_color),
    );
}

/// The minimap panel: translucent scene-background backdrop, accent-bordered,
/// node frames as accent rects, and a viewport indicator showing what the
/// canvas currently displays.
fn paint_minimap(
    painter: &egui::Painter,
    spec: &SceneSpec,
    minimap: &Minimap,
    accent: (u8, u8, u8),
) {
    let (px, py, pw, ph) = minimap.panel;
    let panel = egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(pw, ph));
    painter.rect_filled(
        panel,
        egui::CornerRadius::same(4),
        rgba(spec.background, 215),
    );
    painter.rect(
        panel,
        egui::CornerRadius::same(4),
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, rgb(accent)),
        egui::StrokeKind::Inside,
    );
    for &(x, y, w, h) in &minimap.nodes {
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w.max(1.0), h.max(1.0))),
            egui::CornerRadius::same(1),
            rgb(accent),
        );
    }
    let (vx, vy, vw, vh) = minimap.viewport;
    let vrect = egui::Rect::from_min_size(egui::pos2(vx, vy), egui::vec2(vw, vh));
    painter.rect(
        vrect,
        0.0,
        rgba(accent, 40),
        egui::Stroke::new(1.0, rgb(accent)),
        egui::StrokeKind::Inside,
    );
}
/// Ramp stops of the latency coloring scheme: `graph_edge_latency_0` is the
/// coldest stop, `LATENCY_STOPS-1` the hottest (back edges land there).
/// Shared by the legend painter and this builder's ramp formula
/// `ramp[(latency / max) × (LATENCY_STOPS - 1)]`.
pub const LATENCY_STOPS: usize = 5;

/// Padded-out cluster containers: the union of member node rects grows by
/// `CLUSTER_PAD` on every side so containers never touch the frames inside.
const CLUSTER_PAD: f32 = 8.0;

/// Body radius of a node frame.
const NODE_RADIUS: f32 = 10.0;

/// Stroke width of a cable edge.
pub const EDGE_WIDTH: f32 = 2.0;

/// Node title: the first occurrence of a single-instance circuit is the
/// plain name; among repeated instances the first stays plain and later
/// occurrences carry the zero-based index (`copy`, `copy (1)`, ...).
fn instance_title(name: &str, index: usize, repeats: Option<usize>) -> String {
    match repeats {
        Some(n) if n > 1 && index > 0 => format!("{name} ({index})"),
        _ => name.to_string(),
    }
}

/// Midpoint control of a straight-degree Bézier: the exact center between
/// `start` and `end`. `None` for degenerate (coincident-node) edges.
fn edge_ctrl(start: (f32, f32), end: (f32, f32)) -> Option<(f32, f32)> {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return None;
    }
    Some(((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0))
}
/// Builds the window scene spec from the current `App` graph state (design
/// D3): nodes map through the shared `GraphCamera` (identity fallback when
/// `App.graph_camera` is unset), edges resolve the full color-precedence
/// chain, clusters become padded member unions. `None` when no graph exists
/// or the positional state is inconsistent with the graph.
pub fn build_scene_spec(app: &App, theme: &Theme) -> Option<SceneSpec> {
    let graph = app.graph.as_ref()?;
    if app.graph_positions.len() != graph.nodes.len() {
        return None;
    }
    let camera = app.graph_camera.unwrap_or_default();
    let circuit_store = app.current_circuit_store();

    let diff = if app.diff_showing {
        app.filtered_report()
    } else {
        None
    };

    let filtered = !app.dependency_nodes.is_empty();
    let node_visible = |idx: usize| !filtered || app.dependency_nodes.contains(&idx);
    let edge_visible = |idx: usize| !filtered || app.dependency_edges.contains(&idx);

    // Repeated circuit instances get the numbered title suffix; non-circuit
    // nodes always render their plain name.
    let mut circuit_counts: HashMap<String, usize> = HashMap::new();
    for node in &graph.nodes {
        if node.kind == NodeKind::Circuit {
            *circuit_counts.entry(node.circuit.clone()).or_default() += 1;
        }
    }

    let mut nodes = Vec::with_capacity(graph.nodes.len());
    let mut kept: HashMap<usize, usize> = HashMap::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        if !node_visible(i) {
            continue;
        }
        let (px, py) = camera.world_to_pixel(app.graph_positions[i].0, app.graph_positions[i].1);
        let disabled = app.disabled_circuits.contains(&node.id);
        let highlighted = app.hovered_graph_node == Some(i);
        let selected = app.selected_circuit == Some(node.id.clone());
        let diff_marked = diff.as_ref().map(|report| {
            report.added_nodes.contains(&node.id)
                || report.removed_nodes.contains(&node.id)
                || report.changed_nodes.iter().any(|c| c.id == node.id)
        });

        let title = circuit_store
            .get(&node.id)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if node.kind == NodeKind::Circuit {
                    instance_title(
                        &node.circuit,
                        node.instance_index,
                        circuit_counts.get(&node.circuit).copied(),
                    )
                } else {
                    node.circuit.clone()
                }
            });
        let title = if diff_marked == Some(true) {
            format!("{title} *")
        } else {
            title
        };

        // Border/title token chain: dim beats highlight beats kind frames.
        let dim = theme.rgb(theme.graph_node_dim);
        let (border, label_color, border_width) = if disabled {
            (dim, dim, 1.0)
        } else if highlighted || selected {
            (
                theme.rgb(theme.graph_node_highlight),
                theme.rgb(theme.graph_node_highlight),
                3.0,
            )
        } else {
            let (b, t) = match node.kind {
                NodeKind::Controller => (theme.graph_node_controller, theme.graph_node_controller),
                NodeKind::InputJack => (theme.graph_node_jack_input, theme.graph_node_jack_input),
                NodeKind::OutputJack => {
                    (theme.graph_node_jack_output, theme.graph_node_jack_output)
                }
                NodeKind::Circuit => (theme.graph_node_border, theme.graph_node_title),
            };
            (theme.rgb(b), theme.rgb(t), 1.0)
        };

        let idx = nodes.len();
        kept.insert(i, idx);
        nodes.push(NodeSpec {
            x: px,
            y: py,
            w: GRAPH_WINDOW_NODE_W,
            h: GRAPH_WINDOW_NODE_H,
            radius: NODE_RADIUS,
            fill: theme.rgb(theme.graph_node_fill),
            border,
            border_width,
            label: title,
            label_color,
            circuit: node.circuit.clone(),
            instance_index: node.instance_index,
            input_port: false,
            output_port: false,
        });
    }

    let latency = if app.latency_coloring {
        graph.latency.as_ref()
    } else {
        None
    };

    let mut edges = Vec::with_capacity(graph.edges.len());
    for (i, edge) in graph.edges.iter().enumerate() {
        if !edge_visible(i) {
            continue;
        }
        let Some(s) = edge.source_index(&graph.nodes) else {
            continue;
        };
        let Some(t) = edge.sink_index(&graph.nodes) else {
            continue;
        };
        let (Some(&si), Some(&ti)) = (kept.get(&s), kept.get(&t)) else {
            continue;
        };
        let start_node = &nodes[si];
        let end_node = &nodes[ti];
        let start = (
            start_node.x + start_node.w,
            start_node.y + start_node.h / 2.0,
        );
        let end = (end_node.x, end_node.y + end_node.h / 2.0);
        let Some(ctrl) = edge_ctrl(start, end) else {
            continue;
        };

        let incident_disabled = app.disabled_circuits.contains(&graph.nodes[s].id);
        let incident_unselected = graph.not_selected.contains(&graph.nodes[s].section_index)
            || graph.not_selected.contains(&graph.nodes[t].section_index);
        let has_error = graph
            .validation
            .iter()
            .any(|issue| issue.cable == edge.cable);

        let kind = crate::graph_render::cable_kind(graph, &edge.cable);
        let mut diff_state = None;
        if let Some(report) = &diff {
            if report.added_cables.contains(&edge.cable) {
                diff_state = Some(EdgeDiffState::Added);
            } else if report.removed_cables.contains(&edge.cable) {
                diff_state = Some(EdgeDiffState::Removed);
            } else if report.changed_cables.iter().any(|c| c.cable == edge.cable) {
                diff_state = Some(EdgeDiffState::Changed);
            }
        }

        let mut ramp = None;
        let mut back_edge = false;
        if let Some(data) = latency {
            if let Some(entry) = data.edges.iter().find(|l| l.edge_index == i) {
                back_edge = entry.is_back_edge;
                let stop = if entry.is_back_edge {
                    LATENCY_STOPS - 1
                } else {
                    let den = data.summary.max.max(f32::EPSILON);
                    (clamp01(entry.latency / den) * (LATENCY_STOPS - 1) as f32).round() as usize
                };
                ramp = Some(stop);
            }
        }

        let color = if has_error {
            theme.rgb(theme.graph_edge_error)
        } else if incident_disabled || incident_unselected {
            theme.rgb(theme.graph_edge_dim)
        } else if let Some(state) = diff_state {
            match state {
                EdgeDiffState::Added | EdgeDiffState::Changed => {
                    theme.rgb(theme.graph_edge_diff_added)
                }
                EdgeDiffState::Removed => theme.rgb(theme.graph_edge_diff_removed),
            }
        } else if let Some(stop) = ramp {
            let stops = [
                theme.graph_edge_latency_0,
                theme.graph_edge_latency_1,
                theme.graph_edge_latency_2,
                theme.graph_edge_latency_3,
                theme.graph_edge_latency_4,
            ];
            theme.rgb(stops[stop.min(LATENCY_STOPS - 1)])
        } else if edge.cable.starts_with("_REG:") {
            theme.rgb(theme.graph_edge_register)
        } else {
            match kind {
                CableKind::Control => theme.rgb(theme.graph_edge_control),
                CableKind::Audio => theme.rgb(theme.graph_edge_audio),
                CableKind::Midi => theme.rgb(theme.graph_edge_midi),
                CableKind::Unknown => theme.rgb(theme.graph_edge_unknown),
            }
        };

        nodes[si].output_port = true;
        nodes[ti].input_port = true;
        edges.push(EdgeSpec {
            start,
            end,
            ctrl,
            color,
            width: EDGE_WIDTH,
            kind,
            error: has_error,
            dim: incident_disabled || incident_unselected,
            diff: diff_state,
            latency: ramp.map(|ramp_stop| crate::graph_render::EdgeLatency {
                ramp_stop,
                back_edge,
            }),
        });
    }

    // Cluster containers are padded unions of the kept member node rects in
    // the un-filtered graph; dependency-subset rendering skips them because
    // the member index mapping does not match the full-graph range.
    let clusters = if filtered {
        Vec::new()
    } else {
        graph
            .clusters
            .iter()
            .filter_map(|cluster| {
                let members: Vec<usize> = (cluster.section_range.start..cluster.section_range.end)
                    .filter_map(|i| kept.get(&i).copied())
                    .collect();
                if members.is_empty() {
                    return None;
                }
                let mut x0 = f32::INFINITY;
                let mut y0 = f32::INFINITY;
                let mut x1 = f32::NEG_INFINITY;
                let mut y1 = f32::NEG_INFINITY;
                for &m in &members {
                    let n = &nodes[m];
                    x0 = x0.min(n.x);
                    y0 = y0.min(n.y);
                    x1 = x1.max(n.x + n.w);
                    y1 = y1.max(n.y + n.h);
                }
                Some(ClusterSpec {
                    x: x0 - CLUSTER_PAD,
                    y: y0 - CLUSTER_PAD,
                    w: (x1 + CLUSTER_PAD) - (x0 - CLUSTER_PAD),
                    h: (y1 + CLUSTER_PAD) - (y0 - CLUSTER_PAD),
                    title: cluster.title.clone(),
                    border: theme.rgb(theme.graph_cluster_border),
                    title_color: theme.rgb(theme.graph_cluster_title),
                    member_indices: members,
                })
            })
            .collect()
    };

    Some(SceneSpec {
        background: theme.rgb(theme.graph_canvas_bg),
        nodes,
        edges,
        clusters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_render::{CableKind, EdgeLatency, EdgeSpec, NodeSpec};

    fn scene() -> SceneSpec {
        SceneSpec {
            background: (10, 10, 10),
            nodes: vec![
                NodeSpec {
                    x: 0.0,
                    y: 0.0,
                    w: 200.0,
                    h: 80.0,
                    radius: 8.0,
                    fill: (20, 20, 20),
                    border: (200, 200, 200),
                    border_width: 1.0,
                    label: "copy".into(),
                    label_color: (255, 255, 255),
                    circuit: "copy".into(),
                    instance_index: 0,
                    input_port: false,
                    output_port: true,
                },
                NodeSpec {
                    x: 260.0,
                    y: 0.0,
                    w: 200.0,
                    h: 80.0,
                    radius: 8.0,
                    fill: (20, 20, 20),
                    border: (200, 200, 200),
                    border_width: 1.0,
                    label: "seq".into(),
                    label_color: (255, 255, 255),
                    circuit: "seq".into(),
                    instance_index: 0,
                    input_port: true,
                    output_port: true,
                },
                NodeSpec {
                    x: 500.0,
                    y: 300.0,
                    w: 200.0,
                    h: 80.0,
                    radius: 8.0,
                    fill: (20, 20, 20),
                    border: (200, 200, 200),
                    border_width: 1.0,
                    label: "seq".into(),
                    label_color: (255, 255, 255),
                    circuit: "seq".into(),
                    instance_index: 1,
                    input_port: true,
                    output_port: false,
                },
            ],
            edges: vec![EdgeSpec {
                start: (200.0, 40.0),
                end: (260.0, 40.0),
                ctrl: (230.0, 40.0),
                color: (0, 255, 0),
                width: 2.0,
                kind: CableKind::Audio,
                error: false,
                dim: false,
                diff: None,
                latency: Some(EdgeLatency {
                    ramp_stop: 2,
                    back_edge: false,
                }),
            }],
            clusters: vec![],
        }
    }

    #[test]
    fn camera_pan_shifts_pan() {
        let cam = GraphCamera::default();
        let next = camera_pan(&cam, 12.0, -5.0);
        assert_eq!(next.pan, (cam.pan.0 + 12.0, cam.pan.1 - 5.0));
    }

    #[test]
    fn camera_zoom_about_keeps_anchor_pixel_stable() {
        let cam = GraphCamera::default();
        let anchor = (333.0, 111.0);
        let (wx, wy) = cam.pixel_to_world(anchor.0, anchor.1);
        let next = camera_zoom_about(&cam, 1.5, anchor);
        let (px, py) = next.world_to_pixel(wx, wy);
        assert!((px - anchor.0).abs() < 1e-3 && (py - anchor.1).abs() < 1e-3);
    }

    #[test]
    fn node_at_hits_frame_and_misses_empty() {
        let s = scene();
        assert_eq!(node_at(&s, 10.0, 10.0), Some(0));
        assert_eq!(node_at(&s, 199.0, 79.0), Some(0));
        assert_eq!(node_at(&s, 200.0, 40.0), None); // shared edge is excluded
        assert_eq!(node_at(&s, 999.0, 999.0), None);
    }

    #[test]
    fn nodes_in_rect_selects_intersecting_frames() {
        let s = scene();
        // Span the top row only: nodes 0 and 1.
        assert_eq!(nodes_in_rect(&s, (50.0, -10.0, 600.0, 100.0)), vec![0, 1]);
        // A rect over node 2 alone.
        assert_eq!(nodes_in_rect(&s, (510.0, 310.0, 10.0, 10.0)), vec![2]);
        // No overlap at all.
        assert!(nodes_in_rect(&s, (900.0, 900.0, 10.0, 10.0)).is_empty());
    }

    #[test]
    fn normalize_rect_orders_any_drag_direction() {
        let a = (300.0, 50.0);
        let b = (100.0, 200.0);
        let (x, y, w, h) = normalize_rect(a, b);
        assert_eq!((x, y, w, h), (100.0, 50.0, 200.0, 150.0));
        assert_eq!(normalize_rect(b, a), normalize_rect(a, b));
    }

    #[test]
    fn tooltip_shows_circuit_and_latency_readout() {
        let s = scene();
        // Node 0 has an outgoing edge with ramp_stop 2, no back edge.
        let tip = node_tooltip(&s, 0);
        assert!(tip.contains("copy"));
        assert!(tip.contains("latency 2/4"));
        assert!(tip.contains("0 back edges"));
        // Node 2 has no outgoing edges: label only.
        assert_eq!(node_tooltip(&s, 2), "seq (1)");
    }

    #[test]
    fn tooltip_disambiguates_repeated_circuit_names() {
        let s = scene();
        let tip = node_tooltip(&s, 1);
        assert!(tip.contains("seq (0)"));
    }

    #[test]
    fn minimap_maps_nodes_and_viewport_into_panel() {
        let s = scene();
        // Scene bounds (700×380) overflow the 400×300 canvas, so the map shows.
        let m = minimap_layout(&s, egui::vec2(400.0, 300.0)).unwrap();
        let (px, py, pw, ph) = m.panel;
        assert_eq!((px, py, pw, ph), (10.0, 170.0, 180.0, 120.0));
        // Every node frame lands inside the panel's inner area.
        for &(x, y, w, h) in &m.nodes {
            assert!(x >= px && x + w <= px + pw && y >= py && y + h <= py + ph);
        }
        // The viewport box is the whole canvas mapped into mini space; the
        // scene starts at the origin, so the viewport's top-left is the
        // mapping of (0,0) and must sit inside the panel.
        let (vx, vy, vw, vh) = m.viewport;
        assert!(vx >= px && vy >= py && vx + vw <= px + pw && vy + vh <= py + ph);
        assert!(vw > 0.0 && vh > 0.0);
    }

    #[test]
    fn minimap_hidden_when_layout_fits_canvas() {
        let s = scene();
        // Scene bounds (700×380) fit the 1000×800 canvas: no map needed.
        assert!(minimap_layout(&s, egui::vec2(1000.0, 800.0)).is_none());
    }

    #[test]
    fn next_selection_commits_marquee_and_clears_on_plain_press() {
        let s = scene();
        let mut prev: Vec<usize> = Vec::new();
        // A live marquee over the top row selects nodes 0 and 1.
        let marquee = MarqueeSelection {
            rect: (50.0, -10.0, 600.0, 100.0),
            nodes: nodes_in_rect(&s, (50.0, -10.0, 600.0, 100.0)),
        };
        let dragging = WindowFrame {
            marquee: Some(marquee.clone()),
            primary_pressed: false,
            ..WindowFrame::default()
        };
        prev = next_selection(&dragging, &prev);
        assert_eq!(prev, vec![0, 1]);
        // The release frame has no marquee (primary no longer down) and no
        // fresh press: the committed selection survives.
        let released = WindowFrame::default();
        assert_eq!(next_selection(&released, &prev), vec![0, 1]);
        // A fresh press that is not a marquee (a node grab or empty click)
        // clears the window-local selection.
        let pressed = WindowFrame {
            primary_pressed: true,
            ..WindowFrame::default()
        };
        assert!(next_selection(&pressed, &prev).is_empty());
    }

    #[test]
    fn minimap_empty_scene_is_none() {
        let empty = SceneSpec {
            background: (0, 0, 0),
            nodes: vec![],
            edges: vec![],
            clusters: vec![],
        };
        assert!(minimap_layout(&empty, egui::vec2(800.0, 600.0)).is_none());
    }

    /// A default node frame at a given origin, for tight two-node scenes that
    /// exercise edge-port attribution without dragging the whole fixture in.
    fn node_spec(x: f32, y: f32) -> NodeSpec {
        NodeSpec {
            x,
            y,
            w: 200.0,
            h: 80.0,
            radius: 8.0,
            fill: (20, 20, 20),
            border: (200, 200, 200),
            border_width: 1.0,
            label: "copy".into(),
            label_color: (255, 255, 255),
            circuit: "copy".into(),
            instance_index: 0,
            input_port: false,
            output_port: true,
        }
    }

    #[test]
    fn nearest_node_at_attributes_edge_ports_and_returns_none_in_gap() {
        let s = scene();
        // The edge start sits on node 0's right frame and its end on node 1's
        // left frame, so nearest-node attribution maps them to source and sink.
        assert_eq!(nearest_node_at(&s, 200.0, 40.0), Some(0));
        assert_eq!(nearest_node_at(&s, 260.0, 40.0), Some(1));
        // A point in the 200..=260 gap (wider than the 8px margin) matches no
        // node frame, and far-off points miss every margin.
        assert_eq!(nearest_node_at(&s, 230.0, 40.0), None);
        assert_eq!(nearest_node_at(&s, 1000.0, 1000.0), None);
    }

    #[test]
    fn nearest_node_at_tiebreaks_abutting_nodes_by_centre() {
        // Two nodes share an edge at x = 200. A point just right of the seam
        // is closer to the second node's centre, so it resolves there; the
        // exact seam is equidistant to both centres and the first match wins.
        let s = SceneSpec {
            background: (0, 0, 0),
            nodes: vec![node_spec(0.0, 0.0), node_spec(200.0, 0.0)],
            edges: vec![],
            clusters: vec![],
        };
        assert_eq!(nearest_node_at(&s, 210.0, 40.0), Some(1));
        assert_eq!(nearest_node_at(&s, 200.0, 40.0), Some(0));
    }
}

/// Headless tests for `build_scene_spec` (task 2.4): geometry, camera
/// mapping, styling precedence, diff/latency states, and cluster bounds.
#[cfg(test)]
mod scene_builder_tests {
    use super::*;
    use std::path::Path;

    use crate::app::LabelStore;
    use crate::diff::{ChangedCable, ChangedNode, DiffReport};
    use crate::graph::{Cluster, Graph, GraphEdge, GraphNode, TopologyIssue, TopologySeverity};
    use crate::latency::{EdgeLatency as ModelEdgeLatency, LatencyData, LatencySummary};
    use crate::patch::NodeId;

    fn circuit_node(name: &str, idx: usize, section: usize) -> GraphNode {
        GraphNode {
            id: NodeId::circuit(name, idx),
            kind: NodeKind::Circuit,
            circuit: name.to_string(),
            instance_index: idx,
            section_index: section,
        }
    }

    fn scene_app(graph: Graph, positions: &[(f32, f32)]) -> App {
        let mut app = App::new();
        app.graph = Some(graph);
        app.graph_positions = positions.to_vec();
        app
    }

    /// Two chained circuits: `clocktool` sources `_CLK`, `osc` sinks it.
    fn chain_app() -> App {
        let graph = Graph {
            nodes: vec![circuit_node("clocktool", 0, 0), circuit_node("osc", 0, 1)],
            edges: vec![GraphEdge {
                cable: "_CLK".into(),
                source: NodeId::circuit("clocktool", 0),
                sink: NodeId::circuit("osc", 0),
            }],
            ..Graph::default()
        };
        scene_app(graph, &[(10.0, 20.0), (260.0, 20.0)])
    }

    fn theme() -> &'static Theme {
        crate::theme::active()
    }

    fn spec(app: &App) -> SceneSpec {
        build_scene_spec(app, theme()).expect("scene present")
    }

    #[test]
    fn scene_requires_graph_and_aligned_positions() {
        assert!(build_scene_spec(&App::new(), theme()).is_none());
        let graph = Graph {
            nodes: vec![circuit_node("copy", 0, 0)],
            ..Graph::default()
        };
        let app = scene_app(graph, &[]);
        assert!(build_scene_spec(&app, theme()).is_none());
    }

    #[test]
    fn scene_maps_positions_identity_with_fallback_camera() {
        let app = chain_app();
        let s = spec(&app);
        assert_eq!(s.nodes.len(), 2);
        assert_eq!(s.nodes[0].x, 10.0);
        assert_eq!(s.nodes[0].y, 20.0);
        assert_eq!(s.nodes[0].w, GRAPH_WINDOW_NODE_W);
        assert_eq!(s.nodes[0].h, GRAPH_WINDOW_NODE_H);
    }

    #[test]
    fn scene_maps_positions_through_camera() {
        let mut app = chain_app();
        app.graph_camera = Some(GraphCamera {
            zoom: 2.0,
            pan: (10.0, 20.0),
        });
        let s = spec(&app);
        // world 10 -> 2*10 - 10 = 10; world 20 -> 2*20 - 20 = 20.
        assert_eq!(s.nodes[0].x, 10.0);
        assert_eq!(s.nodes[0].y, 20.0);
        // world 260 -> 2*260 - 10 = 510.
        assert_eq!(s.nodes[1].x, 510.0);
    }

    #[test]
    fn scene_camera_zoom_scales_node_spacing() {
        let identity = spec(&chain_app());
        let mut zoomed = chain_app();
        zoomed.graph_camera = Some(GraphCamera {
            zoom: 2.0,
            pan: (0.0, 0.0),
        });
        let z = spec(&zoomed);
        assert_eq!(z.nodes[0].w, GRAPH_WINDOW_NODE_W);
        assert_eq!(
            z.nodes[1].x - z.nodes[0].x,
            (identity.nodes[1].x - identity.nodes[0].x) * 2.0
        );
    }

    #[test]
    fn scene_node_identity_and_ports_from_incidence() {
        let s = spec(&chain_app());
        assert_eq!(s.nodes[0].circuit, "clocktool");
        assert_eq!(s.nodes[0].instance_index, 0);
        assert!(!s.nodes[0].input_port);
        assert!(s.nodes[0].output_port);
        assert!(s.nodes[1].input_port);
        assert!(!s.nodes[1].output_port);
    }

    #[test]
    fn scene_labels_number_repeated_instances() {
        let graph = Graph {
            nodes: vec![circuit_node("copy", 0, 0), circuit_node("copy", 1, 1)],
            ..Graph::default()
        };
        let s = spec(&scene_app(graph, &[(0.0, 0.0), (260.0, 0.0)]));
        assert_eq!(s.nodes[0].label, "copy");
        assert_eq!(s.nodes[1].label, "copy (1)");
    }

    #[test]
    fn scene_labels_adopt_circuit_label_override() {
        let mut app = chain_app();
        app.current_patch_path = Some(Path::new("fixture.ini").to_path_buf());
        app.label_store
            .patch_labels_mut(Path::new("fixture.ini"))
            .circuits
            .insert(
                LabelStore::encode_node_id("clocktool", 0),
                "Berlin Clock".into(),
            );
        let s = spec(&app);
        assert_eq!(s.nodes[0].label, "Berlin Clock");
    }

    #[test]
    fn scene_node_kind_frames_use_kind_tokens() {
        let graph = Graph {
            nodes: vec![
                circuit_node("copy", 0, 0),
                GraphNode {
                    id: NodeId::Controller("b32".into(), 0),
                    kind: NodeKind::Controller,
                    circuit: "b32".into(),
                    instance_index: 0,
                    section_index: 1,
                },
            ],
            ..Graph::default()
        };
        let app = scene_app(graph, &[(0.0, 0.0), (260.0, 0.0)]);
        let s = spec(&app);
        assert_eq!(
            s.nodes[1].border,
            theme().rgb(theme().graph_node_controller)
        );
        assert_eq!(s.nodes[0].border, theme().rgb(theme().graph_node_border));
    }

    #[test]
    fn scene_hover_and_selection_highlight_nodes() {
        let mut app = chain_app();
        app.hovered_graph_node = Some(1);
        let s = spec(&app);
        assert_eq!(s.nodes[1].border_width, 3.0);
        assert_eq!(s.nodes[1].border, theme().rgb(theme().graph_node_highlight));

        let mut app = chain_app();
        app.selected_circuit = Some(NodeId::circuit("osc", 0));
        let s = spec(&app);
        assert_eq!(s.nodes[1].border, theme().rgb(theme().graph_node_highlight));
    }

    #[test]
    fn scene_disabled_nodes_dim_and_edges_dim_but_not_over_error() {
        let mut app = chain_app();
        app.disabled_circuits
            .insert(NodeId::circuit("clocktool", 0));
        let s = spec(&app);
        assert_eq!(s.nodes[0].border, theme().rgb(theme().graph_node_dim));
        assert!(s.edges[0].dim);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_dim));

        // The red error highlight survives dim.
        let mut app = chain_app();
        app.disabled_circuits
            .insert(NodeId::circuit("clocktool", 0));
        if let Some(graph) = app.graph.as_mut() {
            graph.validation.push(TopologyIssue {
                cable: "_CLK".into(),
                severity: TopologySeverity::Error,
                message: "n to 1".into(),
            });
        }
        let s = spec(&app);
        assert!(s.edges[0].error);
        assert!(s.edges[0].dim);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_error));
    }

    #[test]
    fn scene_edge_geometry_right_center_to_left_center_midpoint_ctrl() {
        let s = spec(&chain_app());
        let e = &s.edges[0];
        let (src, dst) = (&s.nodes[0], &s.nodes[1]);
        assert_eq!(e.start, (src.x + src.w, src.y + src.h / 2.0));
        assert_eq!(e.end, (dst.x, dst.y + dst.h / 2.0));
        assert_eq!(
            e.ctrl,
            ((e.start.0 + e.end.0) / 2.0, (e.start.1 + e.end.1) / 2.0)
        );
    }

    #[test]
    fn scene_edges_skip_zero_length_and_unknown_endpoints() {
        let graph = Graph {
            nodes: vec![circuit_node("copy", 0, 0), circuit_node("copy", 1, 1)],
            edges: vec![
                // Coincident nodes: zero-length edge.
                GraphEdge {
                    cable: "_A".into(),
                    source: NodeId::circuit("copy", 0),
                    sink: NodeId::circuit("copy", 1),
                },
                // Self-loop at one node: also zero-length.
                GraphEdge {
                    cable: "_B".into(),
                    source: NodeId::circuit("copy", 0),
                    sink: NodeId::circuit("copy", 0),
                },
                // Unknown sink: dropped.
                GraphEdge {
                    cable: "_C".into(),
                    source: NodeId::circuit("copy", 0),
                    sink: NodeId::circuit("ghost", 0),
                },
            ],
            ..Graph::default()
        };
        let app = scene_app(graph, &[(0.0, 0.0), (0.0, 0.0)]);
        let s = spec(&app);
        // The unknown-sink edge is dropped; coincident and self-loop edges
        // keep their (rect-rim) geometry: start is the source right-rim, end
        // the sink left-rim, distinct spans.
        assert_eq!(s.edges.len(), 2);
        assert_eq!(s.edges[0].start.0 - s.edges[0].end.0, GRAPH_WINDOW_NODE_W);
        assert!(s.nodes[0].output_port);
        assert!(s.nodes[0].input_port);
    }

    #[test]
    fn scene_filtered_subset_drops_nodes_edges_and_containers() {
        let graph = Graph {
            nodes: vec![
                circuit_node("clocktool", 0, 0),
                circuit_node("osc", 0, 1),
                circuit_node("vca", 0, 2),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_CLK".into(),
                    source: NodeId::circuit("clocktool", 0),
                    sink: NodeId::circuit("osc", 0),
                },
                GraphEdge {
                    cable: "_AUD".into(),
                    source: NodeId::circuit("osc", 0),
                    sink: NodeId::circuit("vca", 0),
                },
            ],
            clusters: vec![Cluster {
                title: "Core".into(),
                section_range: 0..3,
            }],
            ..Graph::default()
        };
        let mut app = scene_app(graph, &[(0.0, 0.0), (260.0, 0.0), (520.0, 0.0)]);
        app.dependency_nodes = vec![0, 1];
        app.dependency_edges = vec![0];
        let s = spec(&app);
        assert_eq!(s.nodes.len(), 2);
        assert_eq!(s.edges.len(), 1);
        assert!(s.clusters.is_empty());
        assert_eq!(s.nodes[0].circuit, "clocktool");
    }

    #[test]
    fn scene_edge_kind_classification() {
        assert_eq!(CableKind::from_circuit("clocktool"), CableKind::Control);
        assert_eq!(CableKind::from_circuit("osc"), CableKind::Audio);
        assert_eq!(CableKind::from_circuit("notesequencer"), CableKind::Midi);

        let s = spec(&chain_app());
        assert_eq!(s.edges[0].kind, CableKind::Control);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_control));
    }

    #[test]
    fn scene_edge_diff_colors_added_removed_changed() {
        let mut app = chain_app();
        app.diff_showing = true;
        app.diff_report = Some(DiffReport {
            added_cables: vec!["_CLK".into()],
            ..Default::default()
        });
        let s = spec(&app);
        assert_eq!(s.edges[0].diff, Some(EdgeDiffState::Added));
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_diff_added));

        let mut app = chain_app();
        app.diff_showing = true;
        app.diff_report = Some(DiffReport {
            removed_cables: vec!["_CLK".into()],
            ..Default::default()
        });
        let s = spec(&app);
        assert_eq!(s.edges[0].diff, Some(EdgeDiffState::Removed));
        assert_eq!(
            s.edges[0].color,
            theme().rgb(theme().graph_edge_diff_removed)
        );

        let mut app = chain_app();
        app.diff_showing = true;
        app.diff_report = Some(DiffReport {
            changed_cables: vec![ChangedCable {
                cable: "_CLK".into(),
                old_sources: vec![],
                new_sources: vec!["_NEW".into()],
                old_sinks: vec![],
                new_sinks: vec![],
            }],
            ..Default::default()
        });
        let s = spec(&app);
        assert_eq!(s.edges[0].diff, Some(EdgeDiffState::Changed));
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_diff_added));
    }

    #[test]
    fn scene_edge_latency_ramp_and_hottest_back_edge() {
        let mut app = chain_app();
        app.latency_coloring = true;
        if let Some(graph) = app.graph.as_mut() {
            graph.latency = Some(LatencyData {
                edges: vec![ModelEdgeLatency {
                    edge_index: 0,
                    latency: 0.5,
                    is_back_edge: false,
                }],
                summary: LatencySummary {
                    avg: 0.5,
                    max: 1.0,
                    back_edge_count: 0,
                },
            });
        }
        let s = spec(&app);
        // 0.5 of max over 5 stops lands on stop 2.
        assert_eq!(s.edges[0].latency.as_ref().unwrap().ramp_stop, 2);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_latency_2));

        let mut app = chain_app();
        app.latency_coloring = true;
        if let Some(graph) = app.graph.as_mut() {
            graph.latency = Some(LatencyData {
                edges: vec![ModelEdgeLatency {
                    edge_index: 0,
                    latency: 0.5,
                    is_back_edge: true,
                }],
                summary: LatencySummary {
                    avg: 0.5,
                    max: 1.0,
                    back_edge_count: 1,
                },
            });
        }
        let s = spec(&app);
        let l = s.edges[0].latency.as_ref().unwrap();
        assert_eq!(l.ramp_stop, LATENCY_STOPS - 1);
        assert!(l.back_edge);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_latency_4));
    }

    #[test]
    fn scene_edge_register_token_beats_kind() {
        let mut app = chain_app();
        if let Some(graph) = app.graph.as_mut() {
            graph.edges[0].cable = "_REG:_VAR".into();
        }
        let s = spec(&app);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_register));
    }

    #[test]
    fn scene_diff_node_marker_and_no_diff_without_overlay() {
        let mut app = chain_app();
        app.diff_showing = true;
        app.diff_report = Some(DiffReport {
            added_nodes: vec![NodeId::circuit("osc", 0)],
            ..Default::default()
        });
        let s = spec(&app);
        assert_eq!(s.nodes[1].label, "osc *");

        // Without the overlay the same report leaves labels untouched.
        let mut app = chain_app();
        app.diff_report = Some(DiffReport {
            added_nodes: vec![NodeId::circuit("osc", 0)],
            ..Default::default()
        });
        app.diff_showing = false;
        let s = spec(&app);
        assert_eq!(s.nodes[1].label, "osc");
    }

    #[test]
    fn scene_cluster_bounds_padded_union_with_title() {
        let graph = Graph {
            nodes: vec![circuit_node("copy", 0, 0), circuit_node("mixer", 0, 1)],
            clusters: vec![Cluster {
                title: "Core".into(),
                section_range: 0..2,
            }],
            ..Graph::default()
        };
        let s = spec(&scene_app(graph, &[(100.0, 100.0), (260.0, 60.0)]));
        assert_eq!(s.clusters.len(), 1);
        let c = &s.clusters[0];
        assert_eq!(c.title, "Core");
        assert_eq!(c.member_indices, vec![0, 1]);
        assert_eq!(c.x, 100.0 - CLUSTER_PAD);
        assert_eq!(c.y, 60.0 - CLUSTER_PAD);
        assert_eq!(c.x + c.w, 260.0 + GRAPH_WINDOW_NODE_W + CLUSTER_PAD);
        assert_eq!(c.border, theme().rgb(theme().graph_cluster_border));
        assert_eq!(c.title_color, theme().rgb(theme().graph_cluster_title));
    }

    #[test]
    fn scene_background_canvas_token_and_shape_counts() {
        let s = spec(&chain_app());
        assert_eq!(s.background, theme().rgb(theme().graph_canvas_bg));
        assert_eq!(s.nodes.len(), 2);
        assert_eq!(s.edges.len(), 1);
        assert!(s.clusters.is_empty());
        // The painter consumes resolved RGB only (design D3): the same invariants the
        // outer `tests` module asserts against synthetic scenes hold here for built ones.
        let _ = ChangedNode {
            id: NodeId::circuit("osc", 0),
            changed_params: vec![],
        };
    }
}

/// Headless regression proof for what the desktop graph window paints: drives
/// [`paint_scene`] through an egui `Context` exactly as `EguiSurface::paint`
/// does, and pins the emitted shapes across the four view states the window
/// reaches — ready (fitted camera + scene), waiting (no scene yet), mid-window
/// (canvas smaller than the fitted scene, so the minimap appears), and zoomed
/// (camera zoom scales node spacing). A camera/scene regression that silently
/// repaints the window black or pushes content off-canvas now fails the suite
/// instead of surfacing only on a live screen (bead droid_tui-69r).
#[cfg(test)]
mod window_paint_tests {
    use super::*;
    use crate::graph::{Graph, GraphEdge, GraphNode};
    use crate::patch::NodeId;

    fn circuit_node(name: &str, idx: usize, section: usize) -> GraphNode {
        GraphNode {
            id: NodeId::circuit(name, idx),
            kind: NodeKind::Circuit,
            circuit: name.to_string(),
            instance_index: idx,
            section_index: section,
        }
    }

    /// Two chained circuits (`clocktool` sources `_CLK`, `osc` sinks it) at
    /// world positions the first-frame fit frames.
    fn chain_app() -> App {
        let graph = Graph {
            nodes: vec![circuit_node("clocktool", 0, 0), circuit_node("osc", 0, 1)],
            edges: vec![GraphEdge {
                cable: "_CLK".into(),
                source: NodeId::circuit("clocktool", 0),
                sink: NodeId::circuit("osc", 0),
            }],
            ..Graph::default()
        };
        let mut app = App::new();
        app.graph = Some(graph);
        app.graph_positions = vec![(10.0, 20.0), (260.0, 20.0)];
        app
    }

    fn theme() -> &'static Theme {
        crate::theme::active()
    }

    /// Shapes `paint_scene` emits, bucketed by kind so the four-state tests can
    /// assert on what the window actually draws (labels, node frames, ports,
    /// edges, arrowheads, minimap) without depending on egui mesh internals.
    struct PaintOutput {
        labels: Vec<String>,
        rects: Vec<egui::epaint::RectShape>,
        circles: usize,
        beziers: usize,
        polygons: usize,
    }

    fn paint(scene: Option<&SceneSpec>, canvas: egui::Vec2) -> PaintOutput {
        paint_with(
            scene,
            canvas,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
                ..Default::default()
            },
        )
    }

    /// `paint` with the pointer parked at `pos`, so `paint_polish` renders the
    /// hover tooltip alongside the scene (the deadlock regression below).
    fn paint_with_pointer(
        scene: Option<&SceneSpec>,
        canvas: egui::Vec2,
        pos: egui::Pos2,
    ) -> PaintOutput {
        paint_with(
            scene,
            canvas,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
                events: vec![egui::Event::PointerMoved(pos)],
                ..Default::default()
            },
        )
    }

    fn paint_with(
        scene: Option<&SceneSpec>,
        canvas: egui::Vec2,
        raw_input: egui::RawInput,
    ) -> PaintOutput {
        let ctx = egui::Context::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_scene(ui.painter(), canvas, scene, ui.ctx(), &[]);
        });
        let mut labels = Vec::new();
        let mut rects = Vec::new();
        let mut circles = 0usize;
        let mut beziers = 0usize;
        let mut polygons = 0usize;
        for cs in &full_output.shapes {
            match &cs.shape {
                egui::epaint::Shape::Text(t) => labels.push(t.galley.text().to_string()),
                egui::epaint::Shape::Rect(r) => rects.push(r.clone()),
                egui::epaint::Shape::Circle(_) => circles += 1,
                egui::epaint::Shape::CubicBezier(_) => beziers += 1,
                egui::epaint::Shape::Path(_) => polygons += 1,
                _ => {}
            }
        }
        full_output.textures_delta.clear();
        PaintOutput {
            labels,
            rects,
            circles,
            beziers,
            polygons,
        }
    }

    /// Left-edge x of each node body fill: the `GRAPH_WINDOW_NODE_W ×
    /// GRAPH_WINDOW_NODE_H` rects with a non-transparent fill (`rect_filled`
    /// node frames). The full-canvas background, minimap panel, and port
    /// circles are a different size or shape, and node borders draw with a
    /// transparent fill, so the size + fill filter isolates the bodies.
    fn node_origins(out: &PaintOutput) -> Vec<f32> {
        let mut xs: Vec<f32> = out
            .rects
            .iter()
            .filter(|r| {
                (r.rect.width() - GRAPH_WINDOW_NODE_W).abs() < 0.5
                    && (r.rect.height() - GRAPH_WINDOW_NODE_H).abs() < 0.5
                    && r.fill != egui::Color32::TRANSPARENT
            })
            .map(|r| r.rect.left())
            .collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs
    }

    #[test]
    fn window_paint_ready_fits_scene_into_canvas() {
        // The window's first frame: a fitted camera frames the whole graph so
        // every node body lands on canvas (the original black-window bug).
        let mut app = chain_app();
        app.graph_camera = Some(graph_window_fit_camera(&app.graph_positions));
        let scene = build_scene_spec(&app, theme()).expect("scene present");
        let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));

        assert!(
            out.labels.iter().any(|l| l.contains("clocktool")),
            "clocktool label: {:?}",
            out.labels
        );
        assert!(
            out.labels.iter().any(|l| l.contains("osc")),
            "osc label: {:?}",
            out.labels
        );
        assert_eq!(out.beziers, 1, "one cable edge");
        assert_eq!(out.polygons, 1, "one direction arrow");
        assert_eq!(out.circles, 2, "output + input port markers");

        let origins = node_origins(&out);
        assert_eq!(origins.len(), 2, "both node bodies drawn: {origins:?}");
        for &x in &origins {
            assert!(
                x >= -0.5 && x + GRAPH_WINDOW_NODE_W <= 1280.5,
                "node body on canvas: {x}"
            );
        }
    }

    /// The hover tooltip must paint without deadlocking: `paint_polish` reads
    /// the pointer under `ctx.input` (the context write lock), and text layout
    /// takes that same lock, so the tooltip has to draw after the input
    /// closure releases it. The old nesting blocked until egui's 10s debug
    /// lock timeout panicked (reported as "g g force-closes the app"); a
    /// regression hangs here for 10s and then panics.
    #[test]
    fn hover_tooltip_paints_without_deadlocking() {
        let app = chain_app();
        let scene = build_scene_spec(&app, theme()).expect("scene present");
        let node = &scene.nodes[0];
        let canvas = egui::vec2(1280.0, 800.0);
        let out = paint_with_pointer(
            Some(&scene),
            canvas,
            egui::pos2(node.x + node.w / 2.0, node.y + node.h / 2.0),
        );
        // The unique circuit name appears once as the node label and a second
        // time inside the tooltip card, so a single hit means no tooltip.
        let hits = out
            .labels
            .iter()
            .filter(|l| l.contains("clocktool"))
            .count();
        assert!(
            hits >= 2,
            "node label + tooltip, got {hits}: {:?}",
            out.labels
        );
    }

    #[test]
    fn window_paint_waiting_draws_nothing() {
        // No graph yet: paint_scene returns before drawing anything, so the
        // swapchain clear color alone shows (the empty/waiting window).
        let out = paint(None, egui::vec2(1280.0, 800.0));
        assert!(out.labels.is_empty());
        assert!(out.rects.is_empty());
        assert_eq!(out.circles, 0);
        assert_eq!(out.beziers, 0);
        assert_eq!(out.polygons, 0);
    }

    #[test]
    fn window_paint_mid_window_shows_minimap() {
        // A mid-sized window smaller than the fitted scene overflows, so the
        // minimap appears bottom-left while the scene content still paints.
        let mut app = chain_app();
        app.graph_camera = Some(graph_window_fit_camera(&app.graph_positions));
        let scene = build_scene_spec(&app, theme()).expect("scene present");
        let out = paint(Some(&scene), egui::vec2(400.0, 300.0));

        assert!(
            out.labels.iter().any(|l| l.contains("clocktool")),
            "content still painted"
        );
        let panel = egui::Rect::from_min_size(egui::pos2(10.0, 170.0), egui::vec2(180.0, 120.0));
        assert!(
            out.rects
                .iter()
                .any(|r| r.rect == panel && r.fill != egui::Color32::TRANSPARENT),
            "minimap panel drawn bottom-left: {:?}",
            out.rects.iter().map(|r| r.rect).collect::<Vec<_>>()
        );
    }

    #[test]
    fn window_paint_zoomed_scales_node_spacing() {
        // Zooming the camera doubles the node spacing on the painted canvas;
        // the window reflects camera zoom rather than a fixed spec layout.
        let identity = {
            let app = chain_app();
            let scene = build_scene_spec(&app, theme()).expect("scene present");
            node_origins(&paint(Some(&scene), egui::vec2(1280.0, 800.0)))
        };
        let mut zoomed_app = chain_app();
        zoomed_app.graph_camera = Some(GraphCamera {
            zoom: 2.0,
            pan: (0.0, 0.0),
        });
        let zoomed = {
            let scene = build_scene_spec(&zoomed_app, theme()).expect("scene present");
            node_origins(&paint(Some(&scene), egui::vec2(1280.0, 800.0)))
        };

        assert_eq!(identity.len(), 2);
        assert_eq!(zoomed.len(), 2);
        let id_gap = identity[1] - identity[0];
        let zoom_gap = zoomed[1] - zoomed[0];
        assert!(
            (zoom_gap - 2.0 * id_gap).abs() < 0.5,
            "zoom doubles spacing: {id_gap} -> {zoom_gap}"
        );
    }
}
#[cfg(test)]
// Small fit test: the camera frames a spread world and carries a finite
// zoom, so the seeded scene is visible in the window.
mod fit_tests {
    #[test]
    fn graph_window_fit_camera_frames_a_spread_world() {
        let positions = vec![(0.0, 0.0), (160.0, 0.0), (320.0, 120.0)];
        let camera = super::graph_window_fit_camera(&positions);
        assert!(camera.zoom.is_finite());
        assert!(camera.zoom > 0.0);
        assert!(camera.pan.0.is_finite() && camera.pan.1.is_finite());
    }
}
