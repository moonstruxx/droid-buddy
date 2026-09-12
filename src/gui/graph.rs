//! Graph canvas painting and interaction for the egui window surface (task
//! 1.2 split: the canvas moved out of the window shell).
//!
//! Everything here either draws from the shared [`SceneSpec`] produced by the
//! `GraphCamera` (one spec pixel = one egui point) or computes pure geometry
//! over it (hit-testing, marquee, minimap, tooltip), so the module tests
//! headless under `cargo test`. [`paint_scene`] is the draw routine the shell
//! dispatches to each frame; the camera helpers (`camera_pan`,
//! `camera_zoom_about`) are the window's bridge to `App::graph_camera`.

use super::{MarqueeSelection, WindowFrame};
use crate::graph_render::{EdgeSpec, GraphCamera, SceneSpec};

/// Draw one scene frame into the window canvas (design D3/D5): opaque
/// background, cluster containers, then cables with direction arrows, then
/// node frames with ports and titles. The spec is pixel-space output of the
/// shared `GraphCamera`, painted 1:1 (one spec pixel = one egui point), so
/// both surfaces show the same view. Colors come only from the resolved spec
/// RGB — never theme tokens below the spec.
pub(super) fn paint_scene(
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

/// Fill the direction arrow at an edge's `end`, mirroring the tiny-skia
/// painter: tangent `B'(1) = 2·(end − ctrl)`, triangle sized from the width.
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
    egui::Color32::from_rgb(r, g, b)
}

fn rgba((r, g, b): (u8, u8, u8), a: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Wheel-zoom sensitivity: exponent multiplier on the scroll delta so a
/// typical wheel tick reads as a gentle zoom step.
pub(super) const ZOOM_SENSITIVITY: f32 = 0.01;
/// Largest factor a single scroll event may apply; bounds wheel zoom so a
/// fast spin cannot blow the camera off the scene.
pub(super) const MAX_ZOOM_STEP: f32 = 1.5;
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
    ctx.input(|i| {
        if let Some(marquee) = frame_marquee(Some(spec), i) {
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
        if let Some(pos) = i.pointer.latest_pos() {
            if let Some(idx) = node_at(spec, pos.x, pos.y) {
                paint_tooltip(painter, spec, canvas, pos, idx);
            }
        }
    });
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
