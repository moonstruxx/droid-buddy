//! Graph canvas painting and interaction for the egui window surface (task
//! 1.2 split: the canvas moved out of the window shell).
//!
//! Everything here either draws from the shared [`SceneSpec`] produced by the
//! `GraphCamera` (one spec pixel = one egui point) or computes pure geometry
//! over it (hit-testing, marquee, minimap, tooltip), so the module tests
//! headless under `cargo test`. [`paint_scene`] is the draw routine the shell
//! dispatches to each frame; the camera helpers (`camera_pan`,
//! `camera_zoom_about`) are the window's bridge to `App::graph_camera`.

use std::collections::{HashMap, HashSet};
// `write!` into labels/titles avoids a per-segment `format!` allocation.
use std::fmt::Write as _;

use super::{MarqueeSelection, WindowFrame};
use crate::app::App;
use crate::diff::DiffReport;
use crate::graph::{Graph, GraphNode, NodeKind};
use crate::graph_group::{Group, GroupedView, ViewEdge, ViewNode};
use crate::graph_render::{
    CableKind, ClusterSpec, EdgeDiffState, EdgeSpec, GraphCamera, NodeSpec, SceneSpec, WorldBounds,
};
use crate::latency::LatencyData;
use crate::patch::NodeId;
use crate::theme::Theme;

/// Clamps a normalized ramp axis position into 0..1.
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Legibility floor for port markers: below this frame height a node paints
/// without its ports, so a zoomed-out graph is not a wall of overlapping
/// chrome (graph-zoom-node-scaling, design decision 3). Node labels use their
/// own rule — see [`fit_label`] and [`NODE_LABEL_SIZE`]
/// (label-guess-and-screen-scale, design decision 4).
const LOD_MIN_NODE_H: f32 = 12.0;
/// Fixed screen-space font size (points) for node labels, independent of the
/// camera zoom: the label never shrinks when zooming out nor grows when
/// zooming in (label-guess-and-screen-scale, design decision 4). Matches the
/// cluster-title base size, so a zoomed-in graph keeps today's appearance.
const NODE_LABEL_SIZE: f32 = 12.0;
/// Monospace advance as a fraction of the font size (egui's monospace face is
/// close to 0.6 em), used to budget the characters that fit a frame at the
/// fixed [`NODE_LABEL_SIZE`] font.
const MONO_ADVANCE: f32 = 0.6;
/// Fixed screen-space font size (points) for cluster titles, independent of the
/// camera zoom — the cluster counterpart of [`NODE_LABEL_SIZE`], and equal to
/// it so a zoomed-in graph keeps today's appearance.
const CLUSTER_TITLE_PX: f32 = 12.0;
/// Proportional-face advance as a fraction of the font size (about 0.55 em per
/// character), used to size the smallest readable title fragment.
const PROP_ADVANCE: f32 = 0.55;
/// Characters a cluster title needs before it still names its group: a shorter,
/// ellipsized fragment names nothing (the argument ADR 52 makes for cell
/// labels).
const CLUSTER_TITLE_MIN_CHARS: usize = 3;
/// One text row's height as a multiple of the font size (egui's proportional
/// face needs about 1.2 em per row).
const LINE_HEIGHT_EM: f32 = 1.2;
/// Left inset of a cluster title from its box's corner.
const CLUSTER_TITLE_LEFT_INSET: f32 = 6.0;
/// Top inset of a cluster title from its box's corner, which the box-room
/// height gate must hold in addition to one title line.
const CLUSTER_TITLE_TOP_INSET: f32 = 3.0;
/// One cluster-title line at [`CLUSTER_TITLE_PX`].
const CLUSTER_TITLE_LINE_H: f32 = CLUSTER_TITLE_PX * LINE_HEIGHT_EM;
/// Shortest cluster box that can hold a title: the top inset plus one title
/// line.
const CLUSTER_TITLE_MIN_H: f32 = CLUSTER_TITLE_LINE_H + CLUSTER_TITLE_TOP_INSET;
/// Narrowest cluster box that can hold a readable title fragment.
const CLUSTER_TITLE_MIN_W: f32 = CLUSTER_TITLE_MIN_CHARS as f32 * PROP_ADVANCE * CLUSTER_TITLE_PX;
/// Minimap node dot floor (raised from 1 px so dots stay visible when the
/// scene is tiny).
const MINIMAP_DOT_MIN: f32 = 2.0;
/// Labels a group's content peek lists before it summarizes the rest as `…+N`
/// (task 5.2): enough to recognize the run, few enough to stay one card row on
/// a normal pane.
const PEEK_LABEL_LIMIT: usize = 8;
/// Characters that end the shared prefix a group's peek abbreviates with: the
/// prefix is cut back to the last of these, so `encoder E1.1` / `encoder E1.24`
/// peek as `encoder · E1.1–E1.24` rather than `encoder E1. · 1–24`.
const PEEK_LABEL_SEPARATORS: [char; 6] = [' ', '\u{b7}', '-', '\u{2013}', ':', '_'];

/// The camera zoom the scene was built with, derived back from the projected
/// node height (`h = layout::NODE_WORLD_H × zoom`). The scene spec carries no
/// zoom field, and every node shares the same world height, so one node is
/// enough; an empty or malformed scene falls back to 1.0. This is what lets
/// the painter apply minimum clamps and level-of-detail thresholds
/// (graph-zoom-node-scaling, decisions 2/3).
fn spec_zoom(spec: &SceneSpec) -> f32 {
    // Every individually drawn node shares the same world height, so its ratio is
    // the camera zoom exactly; a collapsed group's stand-in box is taller than
    // one node body (it holds a stack), so the smallest ratio over the scene is
    // the zoom whenever any individual node is drawn. It only ever understates
    // it for a scene made entirely of group boxes, which affects LOD clamps and
    // arrow sizing, never geometry.
    let zoom = spec
        .nodes
        .iter()
        .map(|n| n.h / crate::layout::NODE_WORLD_H)
        .fold(f32::INFINITY, f32::min);
    if zoom.is_finite() && zoom > 0.0 {
        zoom
    } else {
        1.0
    }
}

/// Fit a node label to its frame at the fixed [`NODE_LABEL_SIZE`] font
/// (label-guess-and-screen-scale, design decision 4): the font never scales
/// with the camera zoom, text wider than the frame is ellipsized to the
/// characters that fit at that fixed size, and a frame narrower than one
/// character of the font yields `None` so the caller omits the label.
/// `None` also for an empty label or a non-positive width.
fn fit_label(label: &str, w: f32) -> Option<(String, f32)> {
    if w <= 0.0 || label.is_empty() {
        return None;
    }
    let char_w = MONO_ADVANCE * NODE_LABEL_SIZE;
    if w < char_w {
        return None; // narrower than one character of the label font
    }
    let max_chars = (w / char_w).floor() as usize;
    if label.chars().count() <= max_chars {
        Some((label.to_string(), NODE_LABEL_SIZE))
    } else {
        Some((ellipsize_to(label, max_chars), NODE_LABEL_SIZE))
    }
}

/// Ellipsize `s` so the result fits `max_chars`: keeps `max_chars - 1`
/// characters and appends `…`, so the total never exceeds the budget. A
/// one-character budget is the ellipsis alone.
fn ellipsize_to(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let keep = max_chars - 1;
    let mut out: String = s.chars().take(keep).collect();
    out.push('\u{2026}');
    out
}

/// Draw one scene frame into the window canvas (design D3/D5): opaque
/// background, cluster containers, then cables with direction arrows, then
/// node frames with ports and titles. The spec is pixel-space output of the
/// shared `GraphCamera`, painted 1:1 (one spec pixel = one egui point), so
/// both surfaces show the same view. Colors come only from the resolved spec
/// RGB — never theme tokens below the spec.
/// Paint one graph scene into the full canvas from the origin (compat
/// wrapper for the single-pane path and the shape tests).
pub(crate) fn paint_scene(
    ui: &mut egui::Ui,
    canvas: egui::Vec2,
    scene: Option<&SceneSpec>,
    selected: &[usize],
) {
    paint_scene_in(
        ui,
        egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
        scene,
        selected,
        None,
    );
}

/// Paint one graph scene clipped into `canvas` (absolute scene coordinates).
/// Each graph pane paints its pane's scene here; the scene builder must have
/// produced coordinates inside the pane, because egui has no painter
/// translate primitive.
///
/// `app` is the state the scene cannot carry: the painter reads it for the
/// hovered collapsed group's content peek (task 5.2), whose labels only the
/// `App` label chain resolves. A caller without one (the compat wrapper, the
/// screenshot tool, the headless shape tests) paints the scene alone.
pub(crate) fn paint_scene_in(
    ui: &mut egui::Ui,
    canvas: egui::Rect,
    scene: Option<&SceneSpec>,
    selected: &[usize],
    app: Option<&App>,
) {
    let Some(spec) = scene else {
        return; // No scene: the swapchain clear color shows through.
    };
    let painter = ui.painter();
    let clipped = painter.with_clip_rect(canvas);
    let painter = &clipped;

    // Camera zoom derived from the projected node height, so chrome scales and
    // level-of-detail thresholds see the same zoom the scene was built at.
    let zoom = spec_zoom(spec);

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
        // Cluster titles are fixed screen-space text, like node labels
        // (ADR 43): the font never scales with the camera. The box decides
        // visibility, measured on its already-projected rect: it needs room
        // for one title line below the top inset, and for a readable fragment
        // of the title. A box without that room stays anonymous instead of
        // drawing an ellipsized sliver that names nothing (ADR 52).
        if !cluster.title.is_empty()
            && rect.height() >= CLUSTER_TITLE_MIN_H
            && rect.width() >= CLUSTER_TITLE_MIN_W
        {
            painter.text(
                rect.left_top() + egui::vec2(CLUSTER_TITLE_LEFT_INSET, CLUSTER_TITLE_TOP_INSET),
                egui::Align2::LEFT_TOP,
                &cluster.title,
                egui::FontId::proportional(CLUSTER_TITLE_PX),
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
        paint_arrow(painter, edge, zoom);
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
        // Port markers are level-of-detail chrome: below the legibility
        // threshold the node paints without them (design decision 3). Labels
        // follow their own rule — fixed font, ellipsized to the frame,
        // hidden below one character of width (label-guess-and-screen-scale,
        // design decision 4).
        let show_chrome = node.h >= LOD_MIN_NODE_H;
        let port_r = (node.h * 0.16).clamp(1.0, 8.0);
        let mid_y = node.y + node.h / 2.0;
        if show_chrome && node.input_port {
            painter.circle_filled(egui::pos2(node.x, mid_y), port_r, rgb(node.border));
        }
        if show_chrome && node.output_port {
            painter.circle_filled(egui::pos2(node.x + node.w, mid_y), port_r, rgb(node.border));
        }
        if let Some((text, px)) = fit_label(&node.label, node.w) {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &text,
                egui::FontId::monospace(px),
                rgb(node.label_color),
            );
        }
        // AccessKit annotation for egui_kittest query-by-label
        let response = ui.allocate_rect(rect, egui::Sense::hover());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &node.label));
    }

    // Task 3.2/3.3: selection, navigation and inspection overlays.
    let ctx = ui.ctx();
    paint_polish(painter, canvas, scene, ctx, selected, app);
}

/// Fill the direction arrow at an edge's `end`, following the scene spec's
/// curve: tangent `B'(1) = 2·(end − ctrl)`, triangle sized from the width and
/// the camera zoom, with small minimums so the direction stays visible at the
/// zoom floor (graph-zoom-node-scaling, design decision 2).
fn paint_arrow(painter: &egui::Painter, edge: &EdgeSpec, zoom: f32) {
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
    let arrow_len = ((8.0 + edge.width * 2.0) * zoom).clamp(2.0, 16.0);
    let half = ((4.0 + edge.width) * zoom).clamp(1.0, 8.0);
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
/// Inset between the minimap panel's outer edge and its map area: the panel
/// edge's padding and, on the interaction side, the drag clamp's bound, so the
/// marquee can only name the map area the dots occupy.
pub(crate) const MINIMAP_INSET: f32 = 4.0;
/// Shortest world span a minimap region zoom frames on either axis. A marquee
/// that maps to a zero-span rect (one pixel wide, or a map whose scale guard
/// collapsed) is widened to this, so the fit divides by a real number instead
/// of the graph's degenerate-span floor.
const MINIMAP_MIN_WORLD_SPAN: f32 = 1.0;
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
/// First-frame camera fit for the desktop graph window, seeded into
/// `App::graph_camera` when it is still unset so the scene frames the whole
/// graph instead of painting the layered seed plane off-viewport (bug: the
/// window opened black). Reused afterwards through the `camera_pan` /
/// `camera_zoom_about` mappings, so user pan/zoom survives. Dependency-filter
/// aware via the caller's positions slice. `viewport` is the visible pane
/// size in points (design D2); `node_world` is the drawn graph's per-axis max
/// node body (`App::graph_fit_node_world`) passed through the shared
/// extent-aware fit, so the whole body of edge nodes stays framed rather than
/// only their top-left position. Pure and window-free.
pub fn graph_window_fit_camera(
    positions: &[(f32, f32)],
    node_world: (f32, f32),
    viewport: (f32, f32),
) -> GraphCamera {
    GraphCamera::fit_to_world_with_nodes(
        WorldBounds::from_positions(positions),
        node_world,
        viewport,
        crate::app::App::GRAPH_MIN_NODE_PX,
    )
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
/// press on a node is a node drag, not a marquee). A press that begins inside
/// the `consume` overlay — the sections sliders panel, passed in as
/// [`sliders_panel_window_rect`] (design D4 pointer priority) — belongs to that
/// overlay and never starts a marquee, the same early return the minimap click
/// uses. `None` when idle.
pub(super) fn frame_marquee(
    scene: Option<&SceneSpec>,
    i: &egui::InputState,
    consume: Option<egui::Rect>,
) -> Option<MarqueeSelection> {
    if !i.pointer.primary_down() {
        return None;
    }
    let origin = i.pointer.press_origin()?;
    if consume.is_some_and(|rect| rect.contains(origin)) {
        return None;
    }
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
pub(crate) struct Minimap {
    pub(crate) panel: (f32, f32, f32, f32),
    pub(crate) viewport: (f32, f32, f32, f32),
    pub(crate) nodes: Vec<(f32, f32, f32, f32)>,
    /// World bounds (bx, by, bw, bh) and inner panel origin (ix, iy) + scale (sx, sy)
    /// for click-to-world conversion: `world_x` = bx + (`click_x` - ix) / sx
    pub(crate) transform: (f32, f32, f32, f32, f32, f32, f32, f32),
}

pub(crate) fn minimap_layout(scene: &SceneSpec, canvas: egui::Rect) -> Option<Minimap> {
    if scene.nodes.is_empty() {
        return None;
    }
    let (mw, mh) = MINIMAP_SIZE;
    let (px, py) = (canvas.min.x + MINIMAP_PAD, canvas.max.y - mh - MINIMAP_PAD);
    let panel = (px, py, mw, mh);
    let (bx, by, bw, bh) = scene_bounds(scene);
    if bw <= 0.0 || bh <= 0.0 {
        return None;
    }
    // The map is drawn whenever the graph has a scene, whatever the zoom: it is
    // the pane's persistent navigation aid (click to pan, drag a region to
    // zoom), so a scene that currently fits the canvas still gets one.
    let inner = (
        px + MINIMAP_INSET,
        py + MINIMAP_INSET,
        mw - 2.0 * MINIMAP_INSET,
        mh - 2.0 * MINIMAP_INSET,
    );
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
    // The visible canvas occupies `canvas` in scene-pixel space.
    let (v0, v0y) = map(canvas.min.x, canvas.min.y);
    let (v1, v1y) = map(canvas.max.x, canvas.max.y);
    // Clamp the viewport box to the map area: when the canvas is larger than the
    // scene (zoomed out so the whole scene fits), the full-canvas box would
    // overrun the panel; clamping makes it read as "the whole scene is
    // visible" — a box filling the map, never a degenerate line. When zoomed
    // in, the box is already a sub-rect and is unchanged. Clamping to the inner
    // rect rather than the panel edge keeps the box off the panel's own border.
    let cx = v0.clamp(ix, ix + iw);
    let cy = v0y.clamp(iy, iy + ih);
    let cx2 = (v1).clamp(ix, ix + iw);
    let cy2 = (v1y).clamp(iy, iy + ih);
    let viewport = (cx, cy, cx2 - cx, cy2 - cy);
    Some(Minimap {
        panel,
        viewport,
        nodes,
        transform: (bx, by, bw, bh, ix, iy, sx, sy),
    })
}

/// The world point under a minimap position: the inverse of
/// [`minimap_layout`]'s `map`.
///
/// `px` is in the space the published transform lives in — canvas points, i.e.
/// window points minus the pane origin — and the result is world (solver)
/// units, so a pointer over the panel names the scene point it covers. A
/// non-positive scale divides by zero, which a degenerate scene can still
/// publish, so such an axis maps to the world bounds' origin instead; callers
/// that must not navigate from a degenerate map gate on the scale themselves.
pub(crate) fn minimap_world_point(
    transform: (f32, f32, f32, f32, f32, f32, f32, f32),
    px: (f32, f32),
) -> (f32, f32) {
    let (bx, by, _bw, _bh, ix, iy, sx, sy) = transform;
    let wx = if sx > 0.0 { bx + (px.0 - ix) / sx } else { bx };
    let wy = if sy > 0.0 { by + (px.1 - iy) / sy } else { by };
    (wx, wy)
}

/// The world rectangle a marquee between two minimap positions names, as
/// `(min_x, min_y, max_x, max_y)` — the shape the camera fit consumes.
///
/// `a` and `b` are the drag's corners in canvas points (the [`Minimap`]
/// transform's space), in either order: the result is normalised, because a
/// drag runs in any direction. A zero-span axis is widened to
/// [`MINIMAP_MIN_WORLD_SPAN`] about its midpoint, so a one-pixel marquee still
/// produces a rect the fit can frame instead of collapsing onto the degenerate
/// span guard.
pub(crate) fn minimap_world_bounds(
    transform: (f32, f32, f32, f32, f32, f32, f32, f32),
    a: (f32, f32),
    b: (f32, f32),
) -> (f32, f32, f32, f32) {
    let (ax, ay) = minimap_world_point(transform, a);
    let (bx, by) = minimap_world_point(transform, b);
    let (min_x, max_x) = (ax.min(bx), ax.max(bx));
    let (min_y, max_y) = (ay.min(by), ay.max(by));
    let span_x = (max_x - min_x).max(MINIMAP_MIN_WORLD_SPAN);
    let span_y = (max_y - min_y).max(MINIMAP_MIN_WORLD_SPAN);
    let (cx, cy) = (f32::midpoint(min_x, max_x), f32::midpoint(min_y, max_y));
    (
        cx - span_x / 2.0,
        cy - span_y / 2.0,
        cx + span_x / 2.0,
        cy + span_y / 2.0,
    )
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
    let _ = write!(
        label,
        " · latency {hot}/4 · {back} back edge{}",
        if back == 1 { "" } else { "s" }
    );
    label
}

/// Draws the canvas polish overlays on top of `paint_scene`: the minimap, the
/// active marquee selection, the committed selection's node highlight, and the
/// hovered node's latency tooltip. Every color derives from the resolved scene
/// spec RGB (never hardcoded). The tooltip is drawn last so it stays readable
/// above marquee, selection, and minimap.
///
/// A hovered collapsed group's content peek takes the tooltip's slot (task 5.2):
/// the pointer then rests on the group's box, so the node card would only name
/// the circuit the box is already labelled with, while the peek names what the
/// box hides. The node card keeps the slot whenever no group is hovered.
fn paint_polish(
    painter: &egui::Painter,
    canvas: egui::Rect,
    scene: Option<&SceneSpec>,
    ctx: &egui::Context,
    selected: &[usize],
    app: Option<&App>,
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
        paint_minimap(
            painter,
            spec,
            &minimap,
            accent,
            app.and_then(|app| app.minimap_drag),
        );
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
    let (marquee, hover, pointer) = ctx.input(|i| {
        let marquee = frame_marquee(Some(spec), i, None);
        let pointer = i.pointer.latest_pos();
        let hover = pointer.and_then(|pos| node_at(spec, pos.x, pos.y).map(|idx| (pos, idx)));
        (marquee, hover, pointer)
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
    // Both cards are laid out after the input read: text layout takes the
    // context write lock again (see the note above).
    if let Some(peek) = pointer.and_then(|pos| group_peek(app, spec, pos)) {
        paint_tooltip_card(
            painter,
            spec,
            canvas,
            peek.anchor,
            &peek.text,
            peek.accent,
            peek.color,
        );
    } else if let Some((pos, idx)) = hover {
        paint_tooltip(painter, spec, canvas, pos, idx);
    }
}

/// The hovered node's tooltip card: an opaque backdrop with the accent
/// border, placed above-right of the cursor and clamped to the canvas.
fn paint_tooltip(
    painter: &egui::Painter,
    spec: &SceneSpec,
    canvas: egui::Rect,
    pos: egui::Pos2,
    node_index: usize,
) {
    let node = &spec.nodes[node_index];
    paint_tooltip_card(
        painter,
        spec,
        canvas,
        pos,
        &node_tooltip(spec, node_index),
        node.border,
        node.label_color,
    );
}

/// One tooltip card (node readout or group content peek): an opaque backdrop
/// with the accent border, placed above-right of the cursor and clamped to the
/// canvas. Both cards share this so the peek lands where the node tooltip
/// already does — the mechanism spec "Hover peeks at contents" reuses.
fn paint_tooltip_card(
    painter: &egui::Painter,
    spec: &SceneSpec,
    canvas: egui::Rect,
    pos: egui::Pos2,
    text: &str,
    accent: (u8, u8, u8),
    color: (u8, u8, u8),
) {
    let font = egui::FontId::proportional(13.0);
    let galley = painter.layout_no_wrap(text.to_string(), font, rgb(color));
    let pad = 6.0;
    let size = galley.size() + egui::vec2(pad * 2.0, pad * 2.0);
    let min_x = (pos.x + 12.0).clamp(canvas.min.x, (canvas.max.x - size.x).max(canvas.min.x));
    let min_y =
        (pos.y - size.y - 8.0).clamp(canvas.min.y, (canvas.max.y - size.y).max(canvas.min.y));
    let min = egui::pos2(min_x, min_y);
    let rect = egui::Rect::from_min_size(min, size);
    painter.rect(
        rect,
        egui::CornerRadius::same(4),
        rgba(spec.background, 255),
        egui::Stroke::new(1.0, rgb(accent)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + egui::vec2(pad, pad), galley, rgb(color));
}

/// A collapsed group's content peek (task 5.2): the line the card shows, the
/// pointer position it is anchored to, and the colour pair it wears — the group
/// box's own resolved border and label tokens, so the peek reads as belonging to
/// the box under the pointer.
struct GroupPeek {
    anchor: egui::Pos2,
    text: String,
    accent: (u8, u8, u8),
    color: (u8, u8, u8),
}

/// The content peek for the collapsed group the pointer rests on.
///
/// `App::hovered_group` already means "the pointer is inside this group's drawn
/// box" — the graph pane's pointer path publishes it from the same world-space
/// extent `group_node_spec` paints (design D4) — so this only has to confirm the
/// box is part of the scene being painted and read the members' labels. An
/// expanded or isolated group draws its members instead of a box, and then
/// there is nothing to peek at. The member labels resolve through the same
/// chain the member titles would have used, so the peek names the nodes exactly
/// as expanding the group would reveal them.
fn group_peek(app: Option<&App>, scene: &SceneSpec, pos: egui::Pos2) -> Option<GroupPeek> {
    let app = app?;
    let group_index = app.hovered_group?;
    let graph = app.graph.as_ref()?;
    let view = scene_groups(app)?;
    let group = view.groups.get(group_index)?;
    // A collapsed group's body is labelled `<circuit> ×<count>` and is the only
    // scene node carrying its circuit, so this finds the box the pointer is on.
    let label = format!("{} \u{d7}{}", group.circuit, group.members.len());
    let body = scene
        .nodes
        .iter()
        .find(|node| node.circuit == group.circuit && node.label == label)?;
    let rect = egui::Rect::from_min_size(egui::pos2(body.x, body.y), egui::vec2(body.w, body.h));
    if !rect.contains(pos) {
        return None;
    }
    let store = app.current_circuit_store();
    let counts = instance_counts(graph);
    let labels = LabelInputs {
        circuit_store: &store,
        circuit_counts: &counts,
    };
    let member_labels: Vec<String> = group
        .members
        .iter()
        .filter_map(|&member| graph.nodes.get(member))
        .map(|node| node_title(app, node, labels, false))
        .collect();
    Some(GroupPeek {
        anchor: pos,
        text: group_peek_line(&member_labels),
        accent: body.border,
        color: body.label_color,
    })
}

/// The peek line for a collapsed group's member labels, in member order.
///
/// Two shapes, per spec "Hover peeks at contents":
///
/// - Members whose labels share a prefix show that prefix and the range their
///   remainders span, `encoder · E1.1–E1.24`. The prefix is the shared run cut
///   back to its last separator, so `encoder E1.1` / `encoder E1.24` peek as
///   `encoder` + `E1.1–E1.24` rather than the unreadable `encoder E1.` + `1–24`.
///   The range carries the member count: a span names how many instances it
///   covers, and it is the shortest form that still says which ones.
/// - Otherwise the first [`PEEK_LABEL_LIMIT`] labels, written in the tooltip's
///   ` · `-separated style, followed by `…+N` for the ones that did not fit.
///
/// Shared-prefix form needs something to name in every label and a range that
/// actually spans: identical labels, or a member whose label is the prefix
/// itself (`copy`, `copy (1)`, … — the numbered-instance case), fall back to the
/// list, which reads correctly there.
pub(crate) fn group_peek_line(labels: &[String]) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let mut shared: Vec<char> = labels[0].chars().collect();
    for label in &labels[1..] {
        let run = label
            .chars()
            .zip(shared.iter())
            .take_while(|(c, s)| c == *s)
            .count();
        shared.truncate(run);
        if shared.is_empty() {
            break;
        }
    }
    let cut = shared
        .iter()
        .rposition(|c| PEEK_LABEL_SEPARATORS.contains(c))
        .map_or(0, |i| i + 1);
    if cut > 0 {
        let prefix = shared[..cut].iter().collect::<String>();
        let remainders: Vec<String> = labels
            .iter()
            .map(|label| label.chars().skip(cut).collect())
            .collect();
        let spans = remainders.len() > 1
            && remainders.iter().all(|r| starts_a_name(r))
            && remainders[0] != *remainders.last().expect("non-empty");
        if spans {
            return format!(
                "{} \u{b7} {}\u{2013}{}",
                prefix.trim_end(),
                remainders[0],
                remainders.last().expect("non-empty")
            );
        }
    }
    let shown: Vec<&str> = labels
        .iter()
        .take(PEEK_LABEL_LIMIT)
        .map(String::as_str)
        .collect();
    let mut line = shown.join(" \u{b7} ");
    let hidden = labels.len().saturating_sub(PEEK_LABEL_LIMIT);
    if hidden > 0 {
        let _ = write!(line, " \u{b7} \u{2026}+{hidden}");
    }
    line
}

/// Whether a label's remainder past the shared prefix still names something: it
/// must start an alphanumeric run. A remainder opening a bracket or punctuation
/// (`(1)`, the numbered-instance suffix) is a list entry, not a ranged instance
/// reference, so those groups keep the list form where a span would read as
/// `copy · (1)–(10)`.
fn starts_a_name(remainder: &str) -> bool {
    remainder.chars().next().is_some_and(char::is_alphanumeric)
}

/// The minimap panel: opaque scene-background backdrop, accent-bordered,
/// node frames as accent rects, an optional region-zoom marquee, and a
/// viewport indicator showing what the canvas currently displays.
///
/// `drag` is the live minimap marquee as `(anchor_x, anchor_y, current_x,
/// current_y)` in window points — the space the pointer arrives in and the
/// space the painter itself draws in, so no origin shift applies — or `None`
/// while no marquee is down.
fn paint_minimap(
    painter: &egui::Painter,
    spec: &SceneSpec,
    minimap: &Minimap,
    accent: (u8, u8, u8),
    drag: Option<(f32, f32, f32, f32)>,
) {
    let (px, py, pw, ph) = minimap.panel;
    let panel = egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(pw, ph));
    painter.rect_filled(
        panel,
        egui::CornerRadius::same(4),
        rgba(spec.background, 255),
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
            egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(w.max(MINIMAP_DOT_MIN), h.max(MINIMAP_DOT_MIN)),
            ),
            egui::CornerRadius::same(1),
            rgb(accent),
        );
    }
    // The region-zoom marquee sits above the dots (it names a part of the map)
    // and below the viewport box (which stays readable through the translucent
    // fill). Clipped to the panel so a drag that ran past the map cannot smear
    // over the scene around it.
    if let Some((ax, ay, bx, by)) = drag {
        let rect =
            egui::Rect::from_two_pos(egui::pos2(ax, ay), egui::pos2(bx, by)).intersect(panel);
        painter.rect_filled(rect, 0.0, rgba(accent, 40));
        painter.rect(
            rect,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, rgb(accent)),
            egui::StrokeKind::Inside,
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

// ── sections-black-hole-solver 4.1/4.3: the sections sliders panel ─────────

/// Sliders panel size `(w, h)` in `egui::Rect` points.
const SLIDERS_PANEL_SIZE: (f32, f32) = (232.0, 132.0);
/// Margin between the sliders panel and the graph pane's edge.
const SLIDERS_PANEL_PAD: f32 = 10.0;
/// Inset of the panel content from its chrome.
const SLIDERS_PANEL_INSET: f32 = 8.0;
/// Height of one panel row (a slider row or the Arrange button row).
const SLIDERS_ROW_H: f32 = 24.0;
/// Vertical gap between panel rows.
const SLIDERS_ROW_GAP: f32 = 4.0;
/// Width of a slider row's label column, left of the slider track.
const SLIDERS_LABEL_W: f32 = 54.0;
/// Font size of a slider row's label.
const SLIDERS_FONT_PX: f32 = 12.0;
/// Upper bound of the slice/section force sliders (task 4.1): the sections
/// springs are stiffnesses of the same order as the cable tension, so both rows
/// share its span with a zero floor. The cable row keeps the solver's own
/// `[TENSION_MIN, TENSION_MAX]` range, so the panel and `Alt+[`/`Alt+]` agree.
const SLIDERS_FORCE_MAX: f32 = 0.5;

/// Upper bound of the force arrangement's Repulsion row (design D7): the gain is
/// a multiplier on the size-aware ramp, whose per-pair push is already capped at
/// the solver's `MAX_DISPLACEMENT` per axis, so only a small multiple of the
/// `1.0` default is a usable span. `3.0` is the ceiling the panel offers, and
/// `0.0` is the loose end (repulsion off).
const REPULSION_GAIN_MAX: f32 = 3.0;

/// Span of the Cohesion row as a multiple of the solver's own default cohesion.
/// The ceiling is derived from [`crate::layout::SolveKnobs::default`] rather than
/// written out again: the default is a measured, retunable value, and a row whose
/// range did not follow it would go stale the moment it moved. The multiplier is
/// `1.0` by measurement, not by taste — cohesion has a narrow legal band and the
/// default sits at its top: below about `0.0063` (of the default's `0.0068`)
/// banner groups interleave, and above the default the separation pass stops
/// clearing its budget (`0.0069` converges only near 380 sweeps, `0.008` does not
/// converge within 512 at all). The row therefore spans "no cohesion" up to the
/// strongest setting whose layout still clears the shipped guarantee, instead of
/// offering a regime where bodies stay overlapped.
const COHESION_SLIDER_SPAN: f32 = 1.0;

/// Floor of the derived Cohesion ceiling. A default of zero would otherwise
/// collapse the row to a degenerate `0.0..=0.0` range that cannot be dragged.
const COHESION_SLIDER_MIN_SPAN: f32 = 0.005;

/// Display (and rounding) precision of the force sliders. Two decimals matches
/// the sections rows and the `0.05` cable-tension step.
const SLIDERS_FORCE_DECIMALS: usize = 2;

/// The Cohesion row carries two digits more: its values are a tenth of the other
/// rows' and its default is a measured number (`0.0068` at the time of writing),
/// so two decimals could not show the knob's own default — and the slider rounds
/// what it cannot show.
const SLIDERS_COHESION_DECIMALS: usize = 4;

/// The Cohesion row's ceiling: [`COHESION_SLIDER_SPAN`]× the solver's default
/// cohesion, floored at [`COHESION_SLIDER_MIN_SPAN`] so the range is always
/// draggable. Read from the solver so retuning the default retunes the row.
fn cohesion_slider_max() -> f32 {
    (crate::layout::SolveKnobs::default().cohesion * COHESION_SLIDER_SPAN)
        .max(COHESION_SLIDER_MIN_SPAN)
}

/// Sliders-panel geometry: the panel chrome rect, one row per force (slice,
/// section, cable in sections mode; repulsion, cohesion, cable in force mode —
/// top to bottom), and the Arrange button row. Pure, like [`Minimap`], so the
/// paint path and the pointer-priority tests share one source of truth. The
/// geometry is mode-independent — only the rows' labels and bindings change
/// (design D7).
pub(crate) struct SlidersPanel {
    pub(crate) rect: egui::Rect,
    /// Slider rows, top to bottom: the arrangement's two force knobs, then the
    /// shared cable row.
    pub(crate) rows: [egui::Rect; 3],
    /// The Arrange button's row.
    pub(crate) arrange: egui::Rect,
}

impl SlidersPanel {
    /// The label column of row `i`: the left edge of the row.
    pub(crate) fn label_rect(&self, i: usize) -> egui::Rect {
        let row = self.rows[i];
        egui::Rect::from_min_size(row.min, egui::vec2(SLIDERS_LABEL_W, row.height()))
    }

    /// The slider track of row `i`: the row right of its label column.
    pub(crate) fn slider_rect(&self, i: usize) -> egui::Rect {
        let row = self.rows[i];
        egui::Rect::from_min_size(
            egui::pos2(row.min.x + SLIDERS_LABEL_W, row.min.y),
            egui::vec2(row.width() - SLIDERS_LABEL_W, row.height()),
        )
    }
}

/// The sliders panel for the current frame, or `None` when it must not show: in
/// the arrangements that have forces (`Force` and `Sections`) with
/// `App::showing_sliders` set, and only in a pane big enough to hold it (a
/// smaller pane would spill the panel over its neighbours). The column
/// arrangement has no forces, so it never lays the panel out — `S` reports that
/// in the status line instead (design D7). Anchored to the pane's top-right
/// corner, mirroring the minimap's bottom-left anchor.
pub(crate) fn sliders_panel_layout(app: &App, pane: egui::Rect) -> Option<SlidersPanel> {
    if !app.showing_sliders || app.layout_mode == crate::config::LayoutMode::Column {
        return None;
    }
    let (w, h) = SLIDERS_PANEL_SIZE;
    if pane.width() < w + 2.0 * SLIDERS_PANEL_PAD || pane.height() < h + 2.0 * SLIDERS_PANEL_PAD {
        return None;
    }
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            pane.max.x - w - SLIDERS_PANEL_PAD,
            pane.min.y + SLIDERS_PANEL_PAD,
        ),
        egui::vec2(w, h),
    );
    let row = |i: usize| {
        let y = rect.min.y + SLIDERS_PANEL_INSET + i as f32 * (SLIDERS_ROW_H + SLIDERS_ROW_GAP);
        egui::Rect::from_min_size(
            egui::pos2(rect.min.x + SLIDERS_PANEL_INSET, y),
            egui::vec2(w - 2.0 * SLIDERS_PANEL_INSET, SLIDERS_ROW_H),
        )
    };
    Some(SlidersPanel {
        rect,
        rows: [row(0), row(1), row(2)],
        arrange: row(3),
    })
}

/// The sliders panel's rect in window space for this frame, when it is shown:
/// the overlay hit rect the canvas input paths consume against before
/// node-drag / node-select / marquee (design D4 pointer priority, task 4.3).
/// Derived the same way [`graph_pane_rect`] derives the pane
/// (`App::pane_geometry`), so the painted panel and the consumed rect agree.
pub fn sliders_panel_window_rect(app: &App, window: egui::Rect) -> Option<egui::Rect> {
    graph_pane_rect(app, window).and_then(|pane| sliders_panel_layout(app, pane).map(|p| p.rect))
}

/// Whether a slider row's response is a real edit by the user rather than the
/// slider re-rounding the knob to its own display precision.
///
/// egui rounds (and clamps) whatever value a `Slider` is handed on *every*
/// frame — two decimals turn a cohesion default of `0.0068` into `0.01` before
/// any input arrives — and it reports that write-back through `changed()`.
/// Treating it as an edit would retune the knob and re-solve the layout on the
/// first painted frame, so the row only follows the pointer or the keyboard: a
/// press or drag on the row, or a key on a focused row.
fn slider_row_edited(ui: &egui::Ui, response: &egui::Response, before: f32, after: f32) -> bool {
    if before == after || !response.changed() {
        return false;
    }
    if response.is_pointer_button_down_on() || response.dragged() {
        return true;
    }
    // Keyboard editing (arrows step the value, Enter/Tab commit a typed one)
    // only counts while the row actually holds focus.
    response.has_focus()
        && ui.input(|i| {
            [
                egui::Key::ArrowLeft,
                egui::Key::ArrowRight,
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
                egui::Key::Enter,
                egui::Key::Tab,
            ]
            .iter()
            .any(|key| i.key_pressed(*key))
        })
}

/// One panel row: the themed label in the row's label column plus a slider
/// showing its numeric value at `decimals`, bound to `value` in the rest of the
/// row. Returns whether the *user* moved the row this frame (see
/// [`slider_row_edited`]), so the caller owns the re-solve policy without a
/// repaint ever counting as an edit.
fn paint_force_slider(
    ui: &mut egui::Ui,
    label_rect: egui::Rect,
    track_rect: egui::Rect,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
) -> bool {
    let t = crate::theme::active();
    ui.painter().text(
        label_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(SLIDERS_FONT_PX),
        t.egui_color(t.graph_node_title),
    );
    let before = *value;
    let (response, draft) = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(track_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                let mut draft = before;
                let response = ui.add(
                    egui::Slider::new(&mut draft, range)
                        .show_value(true)
                        .fixed_decimals(decimals),
                );
                (response, draft)
            },
        )
        .inner;
    if slider_row_edited(ui, &response, before, draft) {
        *value = draft;
        true
    } else {
        false
    }
}

/// Paint the sliders panel into the graph pane (design D4/D7): three force
/// sliders bound to the live `App` values of the active arrangement, plus an
/// Arrange button. In force mode the rows are Repulsion, Cohesion and Cable; in
/// sections mode Slice, Section and Cable (unchanged). A slider change re-solves
/// so the forces are visible while dragging (design D4: rebuild → solve →
/// refit). Arrange follows the arrangement: sections anneals from the current
/// positions (design D5), force re-solves from the seed
/// ([`App::arrange_force`]). Chrome colors come only from `Theme` tokens, in the
/// minimap panel's idiom.
///
/// Pointer priority (task 4.3): the panel registers its own rect as an
/// interactive widget ahead of its children, so it owns every press inside it —
/// the rows stay on top (egui hit-testing prefers the last registered widget at
/// equal distance) and the panel still swallows presses in the gaps between
/// them. The canvas paths additionally consume
/// [`sliders_panel_window_rect`] so the panel wins before node-drag, node
/// select, and marquee.
pub(crate) fn paint_sliders_panel(ui: &mut egui::Ui, app: &mut App, pane: egui::Rect) {
    let Some(panel) = sliders_panel_layout(app, pane) else {
        return;
    };
    let t = crate::theme::active();
    let bg = t.egui_color(t.graph_canvas_bg);
    let accent = t.egui_color(t.graph_cluster_border);
    let text = t.egui_color(t.graph_node_title);
    let fill = t.egui_color(t.graph_node_fill);
    let painter = ui.painter().with_clip_rect(pane);
    // Panel chrome: opaque canvas backdrop + accent border, like the minimap.
    painter.rect_filled(panel.rect, egui::CornerRadius::same(4), bg);
    painter.rect(
        panel.rect,
        egui::CornerRadius::same(4),
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, accent),
        egui::StrokeKind::Inside,
    );
    // Consume the pointer inside the panel before anything painted under it.
    let _ = ui.interact(
        panel.rect,
        ui.id().with("graph_sliders_panel"),
        egui::Sense::click_and_drag(),
    );
    // The first two rows bind the active arrangement's own pair of forces
    // (design D7); the mode is the same one `sliders_panel_layout` gated on, so
    // only the force and sections pairs are reachable here.
    let (first_edited, second_edited) = match app.layout_mode {
        crate::config::LayoutMode::Force => (
            paint_force_slider(
                ui,
                panel.label_rect(0),
                panel.slider_rect(0),
                "Repulsion",
                &mut app.layout_repulsion_force,
                0.0..=REPULSION_GAIN_MAX,
                SLIDERS_FORCE_DECIMALS,
            ),
            paint_force_slider(
                ui,
                panel.label_rect(1),
                panel.slider_rect(1),
                "Cohesion",
                &mut app.layout_cohesion_force,
                0.0..=cohesion_slider_max(),
                SLIDERS_COHESION_DECIMALS,
            ),
        ),
        _ => (
            paint_force_slider(
                ui,
                panel.label_rect(0),
                panel.slider_rect(0),
                "Slice",
                &mut app.layout_slice_force,
                0.0..=SLIDERS_FORCE_MAX,
                SLIDERS_FORCE_DECIMALS,
            ),
            paint_force_slider(
                ui,
                panel.label_rect(1),
                panel.slider_rect(1),
                "Section",
                &mut app.layout_section_force,
                0.0..=SLIDERS_FORCE_MAX,
                SLIDERS_FORCE_DECIMALS,
            ),
        ),
    };
    let cable_edited = paint_force_slider(
        ui,
        panel.label_rect(2),
        panel.slider_rect(2),
        "Cable",
        &mut app.tension,
        crate::layout::TENSION_MIN..=crate::layout::TENSION_MAX,
        SLIDERS_FORCE_DECIMALS,
    );
    let arrange = ui
        .put(
            panel.arrange,
            egui::Button::new(egui::RichText::new("Arrange").color(text))
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, accent)),
        )
        .clicked();
    if first_edited || second_edited || cable_edited {
        app.rebuild_graph();
    }
    if arrange {
        match app.layout_mode {
            crate::config::LayoutMode::Force => {
                app.arrange_force();
            }
            crate::config::LayoutMode::Sections => {
                app.arrange_sections();
            }
            // Unreachable: `sliders_panel_layout` lays out no panel in column
            // mode, so its Arrange button cannot be pressed.
            crate::config::LayoutMode::Column => {}
        }
    }
}

/// Ramp stops of the latency coloring scheme: `graph_edge_latency_0` is the
/// coldest stop, `LATENCY_STOPS-1` the hottest (back edges land there).
/// Shared by the legend painter and this builder's ramp formula
/// `ramp[(latency / max) × (LATENCY_STOPS - 1)]`.
pub const LATENCY_STOPS: usize = 5;

/// Padded-out cluster containers: the union of member node rects grows by
/// `CLUSTER_PAD` on every side so containers never touch the frames inside.
const CLUSTER_PAD: f32 = 8.0;

/// Padding a collapsed group's stand-in box adds on every side of its members'
/// footprint, in world units: the box replaces a stack of frames, so it keeps
/// about one frame's gap — the world-space counterpart of [`CLUSTER_PAD`].
const GROUP_PAD: f32 = 6.0;

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
    Some((f32::midpoint(start.0, end.0), f32::midpoint(start.1, end.1)))
}
/// The egui rect the full graph scene is painted into on the current frame:
/// the class-layout pane holding the Graph view. Derived from
/// `App::pane_geometry` — the ADR 35 single geometry source the render
/// dispatch (`paint_panes`) draws from and publishes as `pane_hit_rects` — so
/// the scene origin, the first-frame fit, the mouse origin, and the paint path
/// all agree on the same rect. `None` when no pane holds the Graph view.
pub fn graph_pane_rect(app: &App, window: egui::Rect) -> Option<egui::Rect> {
    let band = super::to_cell_rect(window);
    app.pane_geometry(band).into_iter().find_map(|(id, cell)| {
        (app.layout.pane(id).view == Some(crate::app::ViewType::Graph)).then(|| {
            // Same cell → egui conversion `paint_panes` uses.
            egui::Rect::from_min_size(
                egui::pos2(f32::from(cell.x), f32::from(cell.y)),
                egui::vec2(f32::from(cell.width), f32::from(cell.height)),
            )
        })
    })
}

/// Builds the full graph scene into `pane` (design D3): nodes map through
/// the shared `GraphCamera` (identity fallback when `App.graph_camera` is
/// unset) with the pan offset by the pane origin — the egui painter has no
/// translate primitive, so scene pixels come out as absolute window
/// coordinates the pane's clip frames. Edges resolve the full
/// color-precedence chain, clusters become padded member unions. `None` when
/// no graph exists or the positional state is inconsistent with the graph.
///
/// Cluster containers are suppressed in sections mode: there the arrangement
/// already reads as density through its member clustering (like a star cluster
/// in the Milky Way, not a drawn box), so the scene carries no enclosing
/// banner-group rectangles. The `Graph.clusters` data is untouched — only its
/// rendering is off. Column and force modes keep the containers unchanged.
pub fn build_scene_spec(app: &App, theme: &Theme, pane: egui::Rect) -> Option<SceneSpec> {
    let groups = scene_groups(app);
    build_scene_spec_with_view(app, theme, pane, groups.as_ref())
}

/// [`build_scene_spec`] against an already-computed grouping answer.
///
/// The seam exists because `App::grouped_view` needs `&mut App` — it lazily
/// rebuilds a cache the settle animation invalidates every frame — while the
/// scene build only paints. A caller that owns `&mut App` pre-warms the cache
/// and hands the borrowed view here, instead of threading `&mut` through a
/// function that only reads. Every current caller goes through
/// [`build_scene_spec`], which derives the same answer from `&App`; see
/// [`scene_groups`] for why it can.
pub(crate) fn build_scene_spec_with_view(
    app: &App,
    theme: &Theme,
    pane: egui::Rect,
    groups: Option<&GroupedView>,
) -> Option<SceneSpec> {
    let graph = app.graph.as_ref()?;
    let mut camera = app.graph_camera.unwrap_or_default();
    // pixel = world × zoom − pan + pane.min: subtracting the pane origin from
    // the pan shifts every spec coordinate into the pane's absolute screen
    // placement, so the pane clip frames the drawn content instead of clipping
    // it to a sliver or off the canvas (a pane in the class layout starts away
    // from the window origin).
    camera.pan = (camera.pan.0 - pane.min.x, camera.pan.1 - pane.min.y);
    let dep_filtered = !app.dependency_nodes.is_empty();
    let inf_filtered = app.influence_filter_active;
    let filtered = dep_filtered || inf_filtered;
    // Sections mode draws no cluster boxes (spec "Cluster boxes are not drawn
    // (sections are visible only through member clustering)"): sections must
    // not appear as drawn objects, only as density. Suppressing the spec here
    // is the single suppression point — every cluster-shaped thing downstream
    // (the painted border, the cluster title, minimap/hit geometry derived from
    // `SceneSpec::clusters`) follows from this list being empty. A filtered
    // render keeps dropping them for every mode, as before.
    let render_clusters = !filtered && app.layout_mode != crate::config::LayoutMode::Sections;
    // Draw the animated (settling) positions when a settle is running; equal to
    // the solved `graph_positions` otherwise. Owned locally so the borrow lives
    // through the scene build.
    let display = app.graph_display_positions();
    // An isolate (design D4) narrows the drawn set to the isolated group's
    // members plus their immediate cable neighbours, which draw dimmed.
    let isolate = groups.and_then(|view| isolate_view(app, graph, view));
    build_scene(
        app,
        theme,
        SceneInput {
            graph,
            positions: &display,
            camera,
            render_clusters,
            groups,
            isolate: isolate.as_ref(),
        },
        |idx| {
            isolate.as_ref().is_none_or(|iso| iso.drawn(idx))
                && (!filtered
                    || (dep_filtered && app.dependency_nodes.contains(&idx))
                    || (inf_filtered && app.influence_nodes.contains(&idx)))
        },
        |idx| {
            !filtered
                || (dep_filtered && app.dependency_edges.contains(&idx))
                || (inf_filtered && app.influence_edges.contains(&idx))
        },
    )
}

/// The grouped view the scene draws through, or `None` when grouping is off or
/// no graph is loaded — which is also the "draw every model node on its own"
/// answer, so the passes below need no separate branch for a disabled view.
///
/// `App` caches this answer behind `&mut self` (`App::grouped_view`) and keeps
/// the cache field private, while the scene build holds `&App`; so the pass is
/// recomputed here from the exact inputs the cache uses — the drawn positions,
/// the layout's per-node world extents, the `[graph]` settings, and the
/// expanded set — through the same pure constructor. Group indices therefore
/// match the ones the interaction layer stores, which is what lets
/// `App::expanded_groups` and `App::isolated_group` name the boxes drawn here.
fn scene_groups(app: &App) -> Option<GroupedView> {
    if !app.graph_grouping.group_enabled {
        return None;
    }
    let graph = app.graph.as_ref()?;
    let positions = app.graph_display_positions();
    let sizes = crate::layout::node_world_sizes(graph);
    let settings = app.group_settings();
    // Isolating a group draws its members individually — the isolate reads as
    // "these nodes, plus their neighbours" (design D4) — so it is expanded for
    // the pass; its outline comes along because the group stays in the view.
    let mut expanded = app.expanded_groups.clone();
    if let Some(group) = app.isolated_group {
        expanded.insert(group);
    }
    Some(crate::graph_group::group_nodes(
        graph, &positions, &sizes, &settings, &expanded,
    ))
}

/// The drawn set of an isolate (design D4): the isolated group's members, plus
/// every node one cable away from any of them, drawn dimmed.
struct Isolate {
    members: HashSet<usize>,
    neighbours: HashSet<usize>,
}

impl Isolate {
    fn drawn(&self, node: usize) -> bool {
        self.members.contains(&node) || self.neighbours.contains(&node)
    }

    fn dimmed(&self, node: usize) -> bool {
        self.neighbours.contains(&node)
    }
}

/// Resolve `App::isolated_group` against the current view: its members and
/// their immediate cable neighbours, over the model graph (direction ignored —
/// an isolate is about what a signal touches, either way). `None` when nothing
/// is isolated, or when the index no longer names a group because the settings
/// changed under it.
fn isolate_view(app: &App, graph: &Graph, view: &GroupedView) -> Option<Isolate> {
    let group = view.groups.get(app.isolated_group?)?;
    let members: HashSet<usize> = group.members.iter().copied().collect();
    let mut neighbours = HashSet::new();
    for edge in &graph.edges {
        let (Some(source), Some(sink)) = (
            edge.source_index(&graph.nodes),
            edge.sink_index(&graph.nodes),
        ) else {
            continue;
        };
        match (members.contains(&source), members.contains(&sink)) {
            (true, false) => {
                neighbours.insert(sink);
            }
            (false, true) => {
                neighbours.insert(source);
            }
            _ => {}
        }
    }
    Some(Isolate {
        members,
        neighbours,
    })
}

/// World-space `(width, height)` node bodies as the renderer draws them
/// (sections-black-hole-solver 3.10). Outside sections mode this is exactly
/// [`crate::layout::node_world_sizes`] — the layout's per-node width estimate
/// that sizes every other mode's frames (ADR 40). In sections mode each width
/// is clamped to [`crate::layout::SECTIONS_LANE_MAX_NODE_W`], the same cap
/// [`crate::layout::sections_lanes`] uses when it sizes the lanes, so a wide
/// node's drawn body never outgrows the lane the solver reserved for it (its
/// label then simply ellipsizes a little more, ADR 43). The cap is scoped to
/// sections mode because the layout is solved once at build time while zoom is
/// a render-time concern: an unconditional cap would change the default column
/// layout.
///
/// One clamp serves both surfaces: the scene's drawn width *is* its spec-space
/// hit extent ([`node_at`], [`nodes_in_rect`], [`nearest_node_at`]), so the
/// tooltip, marquee and edge-port attribution geometry all follow the clamped
/// frame. The terminal/window pointer path tests the same extents in *world*
/// units (`handler::apply_graph_frame`); that path must read its body sizes
/// through this function too, which is why it is `pub(crate)` — a node's raw
/// estimate there would make the hover highlight reach past the drawn frame.
pub(crate) fn drawn_world_sizes(mode: crate::config::LayoutMode, graph: &Graph) -> Vec<(f32, f32)> {
    let mut sizes = crate::layout::node_world_sizes(graph);
    if mode == crate::config::LayoutMode::Sections {
        for (w, _) in &mut sizes {
            *w = w.min(crate::layout::SECTIONS_LANE_MAX_NODE_W);
        }
    }
    sizes
}

/// Shared scene core inputs: the graph to map through `camera`, its solved
/// positions, the render switches, and the grouping state the passes draw
/// through. Bundled so `build_scene` stays under the argument-count lint.
#[derive(Clone, Copy)]
struct SceneInput<'a> {
    graph: &'a Graph,
    positions: &'a [(f32, f32)],
    camera: GraphCamera,
    render_clusters: bool,
    /// The grouped view to draw through, when grouping is on.
    groups: Option<&'a GroupedView>,
    /// The isolate's drawn set, when a group is isolated.
    isolate: Option<&'a Isolate>,
}

/// Which way an optimizer preview moved an element's forward-loop latency.
/// Worse wins when an edge and its sibling disagree, so a regression mark never
/// disappears under an improvement mark.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OptDir {
    Worse,
    Better,
}

/// Fixed render context shared by the scene passes: the state the styling
/// reads, the theme it resolves through, and the zoom the scene is built at.
#[derive(Clone, Copy)]
struct SceneStyle<'a> {
    app: &'a App,
    theme: &'a Theme,
    zoom: f32,
}

/// Per-node geometry, indexed by full-graph index.
#[derive(Clone, Copy)]
struct SceneGeometry<'a> {
    positions: &'a [(f32, f32)],
    world: &'a [(f32, f32)],
    camera: GraphCamera,
}

/// The label inputs a node title resolves through.
#[derive(Clone, Copy)]
struct LabelInputs<'a> {
    circuit_store: &'a HashMap<NodeId, String>,
    circuit_counts: &'a HashMap<String, usize>,
}

/// Everything the node pass reads besides the styling context: the graph, its
/// drawn geometry, the diff overlay, the label chain, and the grouping state.
/// Bundled so `build_nodes` stays under the argument-count lint.
struct NodePass<'a> {
    graph: &'a Graph,
    geometry: SceneGeometry<'a>,
    diff: Option<&'a DiffReport>,
    labels: LabelInputs<'a>,
    groups: Option<&'a GroupedView>,
    isolate: Option<&'a Isolate>,
}

/// Node pass output: the drawn nodes plus the two lookups the edge and cluster
/// passes need — the drawn body for a view node (one model node, or a collapsed
/// group standing in for several), and the drawn node for every model node
/// (which cluster members resolve through, a collapsed member landing on its
/// group's box).
struct BuiltNodes {
    nodes: Vec<NodeSpec>,
    bodies: HashMap<ViewNode, usize>,
    by_node: HashMap<usize, usize>,
}

/// Edge pass output: the drawn edges plus the per-node optimizer direction the
/// border pass applies after the loop.
struct BuiltEdges {
    edges: Vec<EdgeSpec>,
    node_dirs: Vec<Option<OptDir>>,
}

/// Everything the edge colour precedence chain reads, bundled so the chain
/// stays one readable function instead of seven positional arguments.
#[derive(Clone, Copy)]
struct EdgeFlags {
    error: bool,
    dimmed: bool,
    influenced: bool,
    diff: Option<EdgeDiffState>,
    ramp: Option<usize>,
    register: bool,
    kind: CableKind,
}

/// Shared scene core: maps `graph` through `camera`, keeps only visible
/// nodes/edges, and styles each element from `App`'s influence and diff state.
fn build_scene(
    app: &App,
    theme: &Theme,
    input: SceneInput<'_>,
    node_visible: impl Fn(usize) -> bool,
    edge_visible: impl Fn(usize) -> bool,
) -> Option<SceneSpec> {
    let SceneInput {
        graph,
        positions,
        camera,
        render_clusters,
        groups,
        isolate,
    } = input;
    if positions.len() != graph.nodes.len() {
        return None;
    }
    let style = SceneStyle {
        app,
        theme,
        zoom: camera.zoom,
    };
    // World-unit node bodies from the layout's per-node estimate, projected
    // through the camera (graph-zoom-node-scaling decision 1): a node's pixel
    // size is its reserved world size times the zoom, so the column
    // arrangement stays overlap-free at any zoom. Indexed by full-graph index.
    // In sections mode the width is clamped to the lane cap there (task 3.10)
    // via `drawn_world_sizes`; every other mode is unchanged.
    let world = drawn_world_sizes(app.layout_mode, graph);
    let geometry = SceneGeometry {
        positions,
        world: &world,
        camera,
    };
    let diff = if app.diff_showing {
        app.filtered_report()
    } else {
        None
    };
    let circuit_store = app.current_circuit_store();
    let circuit_counts = instance_counts(graph);
    let labels = LabelInputs {
        circuit_store: &circuit_store,
        circuit_counts: &circuit_counts,
    };

    let mut built = build_nodes(
        style,
        &NodePass {
            graph,
            geometry,
            diff: diff.as_ref(),
            labels,
            groups,
            isolate,
        },
        node_visible,
    );
    let edges = build_edges(
        style,
        graph,
        &mut built,
        diff.as_ref(),
        groups,
        isolate,
        edge_visible,
    );
    apply_optimizer_node_diff(style, &mut built.nodes, &edges.node_dirs);
    let mut clusters = if render_clusters {
        build_clusters(style, graph, &built.nodes, &built.by_node)
    } else {
        Vec::new()
    };
    // Expanded groups draw their members as ordinary nodes, so the group itself
    // is only visible as the outline around them (design D5).
    if let Some(view) = groups {
        clusters.extend(build_group_outlines(style, view, &built));
    }

    Some(SceneSpec {
        background: theme.rgb(theme.graph_canvas_bg),
        nodes: built.nodes,
        edges: edges.edges,
        clusters,
    })
}

/// Per-circuit-name instance counts, so repeated instances get the numbered
/// title suffix while non-circuit nodes always render their plain name.
fn instance_counts(graph: &Graph) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for node in &graph.nodes {
        if node.kind == NodeKind::Circuit {
            *counts.entry(node.circuit.clone()).or_default() += 1;
        }
    }
    counts
}

/// Build the visible node specs and the two drawn-node lookups. A grouped view
/// draws one body per collapsed group and one per individual node; without one
/// every model node is drawn on its own.
fn build_nodes(
    style: SceneStyle<'_>,
    pass: &NodePass<'_>,
    node_visible: impl Fn(usize) -> bool,
) -> BuiltNodes {
    let SceneStyle { app, theme, zoom } = style;
    let mut nodes = Vec::with_capacity(pass.graph.nodes.len());
    let mut bodies: HashMap<ViewNode, usize> = HashMap::new();
    let mut by_node: HashMap<usize, usize> = HashMap::new();
    let mut drawn_groups = vec![false; pass.groups.map_or(0, |view| view.groups.len())];
    for (i, node) in pass.graph.nodes.iter().enumerate() {
        // A collapsed group is one drawn body, emitted at its first member's
        // slot: group indices ascend by first member, so the drawn order follows
        // the groups' own ordering and stays deterministic.
        if let Some((g, group)) = collapsed_group(pass.groups, i) {
            if std::mem::replace(&mut drawn_groups[g], true) {
                continue; // an earlier member already drew this box
            }
            if !group.members.iter().any(|&m| node_visible(m)) {
                continue;
            }
            let idx = nodes.len();
            nodes.push(group_node_spec(style, group, pass));
            bodies.insert(ViewNode::Group(g), idx);
            for &member in &group.members {
                by_node.insert(member, idx);
            }
            continue;
        }
        if !node_visible(i) {
            continue;
        }
        let (px, py) = pass
            .geometry
            .camera
            .world_to_pixel(pass.geometry.positions[i].0, pass.geometry.positions[i].1);
        let title = node_title(app, node, pass.labels, node_diff_marked(pass.diff, node));
        let dimmed = pass.isolate.is_some_and(|iso| iso.dimmed(i));
        let (border, label_color, border_width) = node_styling(style, node, i, dimmed);

        let idx = nodes.len();
        bodies.insert(ViewNode::Individual(i), idx);
        by_node.insert(i, idx);
        nodes.push(NodeSpec {
            x: px,
            y: py,
            w: pass.geometry.world[i].0 * zoom,
            h: pass.geometry.world[i].1 * zoom,
            radius: NODE_RADIUS * zoom,
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
    BuiltNodes {
        nodes,
        bodies,
        by_node,
    }
}

/// The group a model node is collapsed into, if any. An expanded group's
/// members map to [`ViewNode::Individual`], so this is exactly "is drawn as a
/// group box".
fn collapsed_group(view: Option<&GroupedView>, node: usize) -> Option<(usize, &Group)> {
    let view = view?;
    match view.nodes.get(node)? {
        ViewNode::Group(g) => Some((*g, view.groups.get(*g)?)),
        ViewNode::Individual(_) => None,
    }
}

/// The stand-in body of a collapsed group (design D1/D5): one world-space box
/// holding the members' `bbox` grown by a member body and [`GROUP_PAD`] on
/// every side. Node positions are top-left corners, so the members' frames
/// reach one body down-right of `bbox`: the box is centred on the frame union —
/// the group's `center` shifted by half a body — which is both what makes it
/// contain every member frame and the same footprint the expanded group's
/// outline encloses. It is labelled `<circuit> ×<count>`, framed in the tokens
/// that circuit's own nodes wear, and carrying the strongest highlight class any
/// member would wear ([`group_styling`], task 6.1). Being an ordinary
/// [`NodeSpec`] it gets the painter's zoom scaling, level-of-detail chrome and
/// label-ellipsis rule unchanged.
fn group_node_spec(style: SceneStyle<'_>, group: &Group, pass: &NodePass<'_>) -> NodeSpec {
    let SceneStyle { theme, zoom, .. } = style;
    let (min_x, min_y, max_x, max_y) = group.bbox;
    let mut member = (0.0_f32, 0.0_f32);
    for &m in &group.members {
        let (w, h) = pass.geometry.world.get(m).copied().unwrap_or_default();
        member = (member.0.max(w), member.1.max(h));
    }
    let world_w = (max_x - min_x) + member.0 + 2.0 * GROUP_PAD;
    let world_h = (max_y - min_y) + member.1 + 2.0 * GROUP_PAD;
    // Positions are node top-left corners, so the members' frames reach one body
    // down-right of `bbox`: the box is centred on the members' *frame* union —
    // `center` shifted by half a body — which is what makes it contain every
    // member frame, and the same footprint the expanded group's outline encloses.
    let centre = (
        group.center.0 + member.0 / 2.0,
        group.center.1 + member.1 / 2.0,
    );
    let (px, py) = pass.geometry.camera.world_to_pixel(centre.0, centre.1);
    let (w, h) = (world_w * zoom, world_h * zoom);
    let (border, label_color, border_width) = group_styling(style, group, pass);
    NodeSpec {
        x: px - w / 2.0,
        y: py - h / 2.0,
        w,
        h,
        radius: NODE_RADIUS * zoom,
        fill: theme.rgb(theme.graph_node_fill),
        border,
        border_width,
        label: format!("{} \u{d7}{}", group.circuit, group.members.len()),
        label_color,
        circuit: group.circuit.clone(),
        instance_index: 0,
        input_port: false,
        output_port: false,
    }
}

/// The structural diff class a collapsed group stands in for: a group wears the
/// diff state of its members the way an edge wears its own (task 6.1).
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupDiffClass {
    Added,
    Removed,
}

/// The strongest highlight class among a collapsed group's members (design D5,
/// task 6.1), as `(border, label colour, border width)`.
///
/// Precedence, highest first — the chain nodes and edges already resolve, so a
/// group answers with the class a member would have shown on its own:
///
/// 1. **Topology error**: any validation finding on a cable incident to a
///    member. Error red outranks every overlay (guardrails), and it is the one
///    class a reader must not miss under a collapsed box.
/// 2. **Diff classification**: any member added/removed/changed in the report
///    the scene shows. Ordered like the edge chain — added, then removed, then
///    changed, which wears the added token exactly as a changed cable does.
/// 3. **Selection or influence wash**: any member selected, or inside the active
///    influence set. One class rather than two because that is how the node path
///    ranks them — [`node_styling`] puts hover, selection and influence on the
///    same highlight token and width — and a live modifier gives it the modifier
///    hue, matching the panels wash.
/// 4. The circuit-kind frame the box wore before: `graph_node_border` over
///    `graph_node_title`.
///
/// The label colour always travels with the class, the rule the node path
/// applies: the class colour is both frame and title, so `<circuit> ×<count>`
/// stays legible under an error, diff or highlight frame instead of keeping a
/// kind title token that no longer belongs to it.
fn group_styling(
    style: SceneStyle<'_>,
    group: &Group,
    pass: &NodePass<'_>,
) -> ((u8, u8, u8), (u8, u8, u8), f32) {
    let SceneStyle { app, theme, zoom } = style;
    let members: Vec<&GraphNode> = group
        .members
        .iter()
        .filter_map(|&member| pass.graph.nodes.get(member))
        .collect();
    // Border widths keep the node path's 1-vs-3 distinction, each scaled by the
    // camera zoom with a floor so the frame never vanishes at the zoom floor.
    let normal_border = (1.0 * zoom).max(0.5);
    let highlight_border = (3.0 * zoom).max(1.0);
    if group_has_topology_error(pass.graph, &members) {
        let error = theme.rgb(theme.graph_edge_error);
        return (error, error, highlight_border);
    }
    if let Some(class) = group_diff_class(pass.diff, &members) {
        let color = match class {
            GroupDiffClass::Added => theme.rgb(theme.graph_edge_diff_added),
            GroupDiffClass::Removed => theme.rgb(theme.graph_edge_diff_removed),
        };
        return (color, color, highlight_border);
    }
    let selected = app
        .selected_circuit
        .as_ref()
        .is_some_and(|id| members.iter().any(|node| &node.id == id));
    let influenced = app
        .modifier_influence
        .as_ref()
        .or(app.influence.as_ref())
        .is_some_and(|state| {
            members
                .iter()
                .any(|node| state.influenced_nodes.contains(&node.id))
        });
    if selected || influenced {
        let highlight = if let Some(token) = app.active_modifier() {
            theme.rgb(crate::theme::modifier_hue(token))
        } else {
            theme.rgb(theme.graph_node_highlight)
        };
        return (highlight, highlight, highlight_border);
    }
    (
        theme.rgb(theme.graph_node_border),
        theme.rgb(theme.graph_node_title),
        normal_border,
    )
}

/// Which structural diff class a collapsed group carries, if any: the strongest
/// class among its members in the report the scene is showing, ordered like the
/// edge chain (added, then removed, then changed — a changed member wears the
/// added token, as a changed cable does). `None` without a diff, which is also
/// why the group keeps its kind frame then.
fn group_diff_class(diff: Option<&DiffReport>, members: &[&GraphNode]) -> Option<GroupDiffClass> {
    let report = diff?;
    if members
        .iter()
        .any(|node| report.added_nodes.contains(&node.id))
    {
        Some(GroupDiffClass::Added)
    } else if members
        .iter()
        .any(|node| report.removed_nodes.contains(&node.id))
    {
        Some(GroupDiffClass::Removed)
    } else if members.iter().any(|node| {
        report
            .changed_nodes
            .iter()
            .any(|changed| changed.id == node.id)
    }) {
        Some(GroupDiffClass::Added)
    } else {
        None
    }
}

/// Whether any cable incident to a member carries a topology-validation finding
/// (design D5: a group carrying an error is visually distinct, and expanding
/// reveals the exact member). `Graph::validation` names cables, so a member is
/// implicated through the cables that touch it.
fn group_has_topology_error(graph: &Graph, members: &[&GraphNode]) -> bool {
    if graph.validation.is_empty() {
        return false;
    }
    let fallible: HashSet<&str> = graph
        .validation
        .iter()
        .map(|issue| issue.cable.as_str())
        .collect();
    graph.edges.iter().any(|edge| {
        fallible.contains(edge.cable.as_str())
            && members
                .iter()
                .any(|node| node.id == edge.source || node.id == edge.sink)
    })
}

/// Label chain (label-guess-and-screen-scale 1.3): stored label → tree-derived
/// guess (the `guessed_labels` cache — never a per-frame walk) → raw circuit
/// name / numbered instance title. A node the active diff touched carries a
/// trailing marker.
fn node_title(app: &App, node: &GraphNode, labels: LabelInputs<'_>, diff_marked: bool) -> String {
    let title = labels
        .circuit_store
        .get(&node.id)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| app.guessed_labels.get(&node.id).cloned())
        .unwrap_or_else(|| {
            if node.kind == NodeKind::Circuit {
                instance_title(
                    &node.circuit,
                    node.instance_index,
                    labels.circuit_counts.get(&node.circuit).copied(),
                )
            } else {
                node.circuit.clone()
            }
        });
    if diff_marked {
        format!("{title} *")
    } else {
        title
    }
}

/// Whether the active diff touched this node, so its title carries the marker.
fn node_diff_marked(diff: Option<&DiffReport>, node: &GraphNode) -> bool {
    diff.is_some_and(|report| {
        report.added_nodes.contains(&node.id)
            || report.removed_nodes.contains(&node.id)
            || report.changed_nodes.iter().any(|c| c.id == node.id)
    })
}

/// Border/title token chain: dim beats highlight beats kind frames. With an
/// active influence the influenced border/title token marks the influenced set
/// and the rest dims; hover/selection interaction feedback stays highest. While
/// a modifier is active (latched or held) the influence hue is the per-token
/// `modifier_hue`, matching the panels wash (design D). `dimmed` is the
/// isolate's neighbour set (design D4), which dims exactly like a disabled
/// circuit so an isolate reads as "these members, those neighbours".
fn node_styling(
    style: SceneStyle<'_>,
    node: &GraphNode,
    index: usize,
    dimmed: bool,
) -> ((u8, u8, u8), (u8, u8, u8), f32) {
    let SceneStyle { app, theme, zoom } = style;
    let dim = theme.rgb(theme.graph_node_dim);
    let highlight = if let Some(tok) = app.active_modifier() {
        theme.rgb(crate::theme::modifier_hue(tok))
    } else {
        theme.rgb(theme.graph_node_highlight)
    };
    let influenced = app
        .modifier_influence
        .as_ref()
        .or(app.influence.as_ref())
        .is_some_and(|s| s.influenced_nodes.contains(&node.id));
    // Border widths keep the 1-vs-3 distinction but scale with the camera zoom,
    // each with a minimum so the outline never vanishes at the zoom floor
    // (graph-zoom-node-scaling decision 2).
    let normal_border = (1.0 * zoom).max(0.5);
    let highlight_border = (3.0 * zoom).max(1.0);
    if dimmed || app.disabled_circuits.contains(&node.id) {
        (dim, dim, normal_border)
    } else if app.hovered_graph_node == Some(index)
        || app.selected_circuit == Some(node.id.clone())
        || influenced
    {
        (highlight, highlight, highlight_border)
    } else if app.modifier_influence.is_some() || app.influence.is_some() {
        (dim, dim, normal_border)
    } else {
        let (border_token, title_token) = match node.kind {
            NodeKind::Controller => (theme.graph_node_controller, theme.graph_node_controller),
            NodeKind::InputJack => (theme.graph_node_jack_input, theme.graph_node_jack_input),
            NodeKind::OutputJack => (theme.graph_node_jack_output, theme.graph_node_jack_output),
            NodeKind::Circuit => (theme.graph_node_border, theme.graph_node_title),
        };
        (
            theme.rgb(border_token),
            theme.rgb(title_token),
            normal_border,
        )
    }
}

/// Build the visible edge specs, set the port markers on the nodes they touch,
/// and accumulate each node's optimizer direction.
///
/// Edges travel from the model graph to the drawn bodies through the grouped
/// view (design D3): both endpoints resolve to their view node, a cable whose
/// ends land inside one collapsed group is not drawn at all, and the rest
/// deduplicate onto one arrow per drawn pair — the first cable reaching a pair
/// owns it, so the aggregate fan-out of a stack costs one edge, not one per
/// member. Without a grouped view every model node is its own body and nothing
/// changes.
fn build_edges(
    style: SceneStyle<'_>,
    graph: &Graph,
    built: &mut BuiltNodes,
    diff: Option<&DiffReport>,
    groups: Option<&GroupedView>,
    isolate: Option<&Isolate>,
    edge_visible: impl Fn(usize) -> bool,
) -> BuiltEdges {
    let SceneStyle { app, theme, zoom } = style;
    let latency = if app.latency_coloring {
        graph.latency.as_ref()
    } else {
        None
    };
    let mut node_dirs: Vec<Option<OptDir>> = vec![None; built.nodes.len()];
    let mut edges = Vec::with_capacity(graph.edges.len());
    let mut drawn_pairs: HashSet<ViewEdge> = HashSet::new();
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
        let (source, sink) = match groups {
            Some(view) => {
                let (source, sink) = (view.nodes[s], view.nodes[t]);
                // Cables entirely inside one collapsed group have nothing to
                // draw between; the group's own body is not a loop.
                if matches!((source, sink), (ViewNode::Group(a), ViewNode::Group(b)) if a == b) {
                    continue;
                }
                if !drawn_pairs.insert(ViewEdge { source, sink }) {
                    continue; // an earlier cable already owns this drawn pair
                }
                (source, sink)
            }
            None => (ViewNode::Individual(s), ViewNode::Individual(t)),
        };
        let (Some(&si), Some(&ti)) = (built.bodies.get(&source), built.bodies.get(&sink)) else {
            continue;
        };
        let start = (
            built.nodes[si].x + built.nodes[si].w,
            built.nodes[si].y + built.nodes[si].h / 2.0,
        );
        let end = (
            built.nodes[ti].x,
            built.nodes[ti].y + built.nodes[ti].h / 2.0,
        );
        let Some(ctrl) = edge_ctrl(start, end) else {
            continue;
        };

        let incident_disabled = app.disabled_circuits.contains(&graph.nodes[s].id);
        let incident_unselected = graph.not_selected.contains(&graph.nodes[s].section_index)
            || graph.not_selected.contains(&graph.nodes[t].section_index);
        // An isolate's dimmed neighbours (design D4) dim their cables too.
        let incident_isolated = isolate.is_some_and(|iso| iso.dimmed(s) || iso.dimmed(t));
        let incident_dimmed = incident_disabled || incident_unselected || incident_isolated;
        let has_error = graph
            .validation
            .iter()
            .any(|issue| issue.cable == edge.cable);
        let kind = crate::graph_render::cable_kind(graph, &edge.cable);
        let opt_dir = edge_opt_dir(app, graph, i, &edge.cable);
        if let Some(dir) = opt_dir {
            for &j in &[si, ti] {
                match (node_dirs[j], dir) {
                    (Some(OptDir::Worse), _) | (Some(OptDir::Better), OptDir::Better) => {}
                    (None, d) => node_dirs[j] = Some(d),
                    (Some(OptDir::Better), OptDir::Worse) => node_dirs[j] = Some(OptDir::Worse),
                }
            }
        }
        let diff_state = edge_diff_state(diff, &edge.cable);
        let (ramp, back_edge) = edge_ramp(latency, i);

        let influenced = app
            .modifier_influence
            .as_ref()
            .or(app.influence.as_ref())
            .is_some_and(|s| s.influenced_edges.contains(&edge.cable));
        let mut color = edge_color(
            style,
            EdgeFlags {
                error: has_error,
                dimmed: incident_dimmed,
                influenced,
                diff: diff_state,
                ramp,
                register: edge.cable.starts_with("_REG:"),
                kind,
            },
        );
        // Optimizer-preview diff overrides the kind/latency color (but sits
        // below error/influence/disabled above): latency got worse = red,
        // better = blue (optimizer-click-focus).
        if let Some(dir) = opt_dir {
            color = match dir {
                OptDir::Worse => theme.rgb(theme.graph_edge_diff_before),
                OptDir::Better => theme.rgb(theme.graph_edge_diff_after),
            };
        }

        built.nodes[si].output_port = true;
        built.nodes[ti].input_port = true;
        edges.push(EdgeSpec {
            start,
            end,
            ctrl,
            color,
            width: if influenced {
                ((EDGE_WIDTH + 1.0) * zoom).max(0.75)
            } else {
                (EDGE_WIDTH * zoom).max(0.5)
            },
            kind,
            error: has_error,
            dim: incident_dimmed || (app.influence.is_some() && !influenced),
            diff: diff_state,
            latency: ramp.map(|ramp_stop| crate::graph_render::EdgeLatency {
                ramp_stop,
                back_edge,
            }),
        });
    }
    BuiltEdges { edges, node_dirs }
}

/// Optimizer-preview diff direction for one edge (optimizer-click-focus):
/// compare the current forward-loop latency against the captured file-order
/// baseline by cable name. `None` when the diff is inactive or this edge is
/// unchanged.
fn edge_opt_dir(app: &App, graph: &Graph, edge_index: usize, cable: &str) -> Option<OptDir> {
    let active = app
        .optimizer
        .as_ref()
        .is_some_and(|s| s.previewing.is_some())
        && app.optimizer_diff_state.is_some();
    if !active {
        return None;
    }
    let state = app.optimizer_diff_state.as_ref()?;
    if edge_index >= state.before_latency.len()
        || state
            .before_cable
            .get(edge_index)
            .is_none_or(|c| c.as_str() != cable)
    {
        return None;
    }
    let current = graph
        .latency
        .as_ref()
        .and_then(|lat| lat.edges.iter().find(|l| l.edge_index == edge_index))
        .map_or(0.0, |l| l.latency);
    let delta = current - state.before_latency[edge_index];
    if delta > 1e-6 {
        Some(OptDir::Worse)
    } else if delta < -1e-6 {
        Some(OptDir::Better)
    } else {
        None
    }
}

/// Structural diff classification of one cable, if a diff is showing.
fn edge_diff_state(diff: Option<&DiffReport>, cable: &str) -> Option<EdgeDiffState> {
    let report = diff?;
    if report.added_cables.iter().any(|c| c.as_str() == cable) {
        Some(EdgeDiffState::Added)
    } else if report.removed_cables.iter().any(|c| c.as_str() == cable) {
        Some(EdgeDiffState::Removed)
    } else if report.changed_cables.iter().any(|c| c.cable == cable) {
        Some(EdgeDiffState::Changed)
    } else {
        None
    }
}

/// Latency-ramp stop for one edge and whether it is a back edge (which always
/// sits at the hot end of the ramp).
fn edge_ramp(latency: Option<&LatencyData>, edge_index: usize) -> (Option<usize>, bool) {
    let Some(data) = latency else {
        return (None, false);
    };
    let Some(entry) = data.edges.iter().find(|l| l.edge_index == edge_index) else {
        return (None, false);
    };
    let stop = if entry.is_back_edge {
        LATENCY_STOPS - 1
    } else {
        let den = data.summary.max.max(f32::EPSILON);
        (clamp01(entry.latency / den) * (LATENCY_STOPS - 1) as f32).round() as usize
    };
    (Some(stop), entry.is_back_edge)
}

/// Stroke colour precedence: topology error, then disabled/deselected, then the
/// influenced set, then a dimmed rest of an influence, then the structural diff,
/// then the latency ramp or register edge, then the cable kind.
fn edge_color(style: SceneStyle<'_>, flags: EdgeFlags) -> (u8, u8, u8) {
    let SceneStyle { app, theme, .. } = style;
    if flags.error {
        // Error red outranks influence (guardrails).
        theme.rgb(theme.graph_edge_error)
    } else if flags.dimmed {
        theme.rgb(theme.graph_edge_dim)
    } else if flags.influenced {
        if let Some(tok) = app.active_modifier() {
            theme.rgb(crate::theme::modifier_hue(tok))
        } else {
            theme.rgb(theme.graph_edge_highlight)
        }
    } else if app.modifier_influence.is_some() || app.influence.is_some() {
        theme.rgb(theme.graph_edge_dim)
    } else if let Some(state) = flags.diff {
        match state {
            EdgeDiffState::Added | EdgeDiffState::Changed => theme.rgb(theme.graph_edge_diff_added),
            EdgeDiffState::Removed => theme.rgb(theme.graph_edge_diff_removed),
        }
    } else {
        base_edge_color(theme, flags)
    }
}

/// The undecorated stroke colour: register edges and the latency ramp override
/// the cable-kind colour.
fn base_edge_color(theme: &Theme, flags: EdgeFlags) -> (u8, u8, u8) {
    if let Some(stop) = flags.ramp {
        let stops = [
            theme.graph_edge_latency_0,
            theme.graph_edge_latency_1,
            theme.graph_edge_latency_2,
            theme.graph_edge_latency_3,
            theme.graph_edge_latency_4,
        ];
        return theme.rgb(stops[stop.min(LATENCY_STOPS - 1)]);
    }
    if flags.register {
        return theme.rgb(theme.graph_edge_register);
    }
    match flags.kind {
        CableKind::Control => theme.rgb(theme.graph_edge_control),
        CableKind::Audio => theme.rgb(theme.graph_edge_audio),
        CableKind::Midi => theme.rgb(theme.graph_edge_midi),
        CableKind::Unknown => theme.rgb(theme.graph_edge_unknown),
    }
}

/// Optimizer-preview diff: color each node's border by its aggregated direction
/// (worse beats better). Unchanged nodes keep their resolved border
/// (optimizer-click-focus).
fn apply_optimizer_node_diff(
    style: SceneStyle<'_>,
    nodes: &mut [NodeSpec],
    dirs: &[Option<OptDir>],
) {
    let SceneStyle { theme, zoom, .. } = style;
    for (j, node) in nodes.iter_mut().enumerate() {
        if let Some(dir) = dirs[j] {
            node.border = match dir {
                OptDir::Worse => theme.rgb(theme.graph_node_diff_before),
                OptDir::Better => theme.rgb(theme.graph_node_diff_after),
            };
            node.border_width = (3.0 * zoom).max(1.0);
        }
    }
}

/// Cluster containers are padded unions of the kept member node rects in the
/// un-filtered graph; a filtered render (dependency or influence subset) skips
/// them because the member index mapping does not match the full-graph range.
/// `by_node` resolves a model index to its drawn node, so under grouping a
/// banner group containing a collapsed group encloses that group's box.
fn build_clusters(
    style: SceneStyle<'_>,
    graph: &Graph,
    nodes: &[NodeSpec],
    by_node: &HashMap<usize, usize>,
) -> Vec<ClusterSpec> {
    let SceneStyle { theme, zoom, .. } = style;
    let cluster_pad = (CLUSTER_PAD * zoom).max(2.0);
    graph
        .clusters
        .iter()
        .filter_map(|cluster| {
            let members = union_of(
                nodes,
                (cluster.section_range.start..cluster.section_range.end)
                    .filter_map(|i| by_node.get(&i).copied()),
                cluster_pad,
            )?;
            Some(ClusterSpec {
                x: members.0,
                y: members.1,
                w: members.2,
                h: members.3,
                title: cluster.title.clone(),
                border: theme.rgb(theme.graph_cluster_border),
                title_color: theme.rgb(theme.graph_cluster_title),
                member_indices: members.4,
            })
        })
        .collect()
}

/// The outline around an expanded group (design D5: expand in place draws the
/// member nodes with a group outline). Drawn in the cluster-container style so
/// it reads as a container, in the tokens the group's circuit nodes wear, and
/// titled with the same `<circuit> ×<count>` label the collapsed box carries.
/// A collapsed group is already one drawn body and gets no outline.
///
/// The outline wraps the members' own drawn rects — [`Group::members_bbox`]
/// projected through the camera — and never [`Group::bbox`]: that one is the
/// collapsed body and compacts to a nominal box for a spread type, which would
/// leave the outline floating between the member bodies instead of around them
/// (ADR 40: the extent the pointer hit-tests in
/// `handler::group_outline_box` is the same one).
fn build_group_outlines(
    style: SceneStyle<'_>,
    view: &GroupedView,
    built: &BuiltNodes,
) -> Vec<ClusterSpec> {
    let SceneStyle { theme, zoom, .. } = style;
    let pad = (CLUSTER_PAD * zoom).max(2.0);
    let mut outlines = Vec::new();
    for (g, group) in view.groups.iter().enumerate() {
        if group
            .members
            .iter()
            .any(|&m| view.nodes.get(m) == Some(&ViewNode::Group(g)))
        {
            continue;
        }
        let Some(union) = union_of(
            &built.nodes,
            group
                .members
                .iter()
                .filter_map(|m| built.by_node.get(m).copied()),
            pad,
        ) else {
            continue;
        };
        outlines.push(ClusterSpec {
            x: union.0,
            y: union.1,
            w: union.2,
            h: union.3,
            title: format!("{} \u{d7}{}", group.circuit, group.members.len()),
            border: theme.rgb(theme.graph_node_border),
            title_color: theme.rgb(theme.graph_node_title),
            member_indices: union.4,
        });
    }
    outlines
}

/// Padded union of the drawn rects at `members`, with the member indices it
/// enclosed. `None` when nothing was drawn (every member filtered out). The
/// indices are sorted and deduplicated so a collapsed group, whose members all
/// resolve to its one box, names that box once.
fn union_of(
    nodes: &[NodeSpec],
    members: impl Iterator<Item = usize>,
    pad: f32,
) -> Option<(f32, f32, f32, f32, Vec<usize>)> {
    let mut indices: Vec<usize> = members.collect();
    indices.sort_unstable();
    indices.dedup();
    indices.retain(|&m| m < nodes.len());
    if indices.is_empty() {
        return None;
    }
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for &m in &indices {
        let n = &nodes[m];
        x0 = x0.min(n.x);
        y0 = y0.min(n.y);
        x1 = x1.max(n.x + n.w);
        y1 = y1.max(n.y + n.h);
    }
    Some((
        x0 - pad,
        y0 - pad,
        (x1 + pad) - (x0 - pad),
        (y1 + pad) - (y0 - pad),
        indices,
    ))
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
        let first = (300.0, 50.0);
        let second = (100.0, 200.0);
        let (left, top, width, height) = normalize_rect(first, second);
        assert_eq!((left, top, width, height), (100.0, 50.0, 200.0, 150.0));
        assert_eq!(normalize_rect(second, first), normalize_rect(first, second));
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
        let m = minimap_layout(
            &s,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)),
        )
        .unwrap();
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
    fn minimap_shown_even_when_the_layout_fits() {
        let s = scene();
        // Scene bounds (700×380) fit the 1000×800 canvas: the map is drawn all
        // the same — it is the pane's persistent navigation aid, and a
        // click/drag region zoom must work at any zoom level.
        let m = minimap_layout(
            &s,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0)),
        )
        .expect("the minimap is always laid out for a scene");
        let (px, py, pw, ph) = m.panel;
        assert_eq!((px, py, pw, ph), (10.0, 670.0, 180.0, 120.0));
        // The whole scene is visible, so the viewport box fills the map area
        // (inset by MINIMAP_INSET) rather than collapsing to a line.
        let (vx, vy, vw, vh) = m.viewport;
        assert!((vx - (px + MINIMAP_INSET)).abs() < 0.5, "vx: {vx}");
        assert!((vy - (py + MINIMAP_INSET)).abs() < 0.5, "vy: {vy}");
        assert!((vw - (pw - 2.0 * MINIMAP_INSET)).abs() < 0.5, "vw: {vw}");
        assert!((vh - (ph - 2.0 * MINIMAP_INSET)).abs() < 0.5, "vh: {vh}");
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
    fn minimap_world_point_inverts_the_map() {
        // The published transform is the layout's own `map` inverted: the inner
        // panel's origin is the world bounds' origin, and a panel pixel is
        // `1/scale` world units.
        let s = scene();
        let m = minimap_layout(
            &s,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)),
        )
        .unwrap();
        let (bx, by, _bw, _bh, ix, iy, sx, sy) = m.transform;
        assert_eq!(minimap_world_point(m.transform, (ix, iy)), (bx, by));
        let (wx, wy) = minimap_world_point(m.transform, (ix + 7.0, iy + 3.0));
        assert!((wx - (bx + 7.0 / sx)).abs() < 1e-3, "wx: {wx}");
        assert!((wy - (by + 3.0 / sy)).abs() < 1e-3, "wy: {wy}");
        // Round trip through the forward map: the inverse returns the world
        // point the panel position was built from.
        let (world, px) = ((123.0_f32, 45.0_f32), (ix + 123.0 * sx, iy + 45.0 * sy));
        let got = minimap_world_point(m.transform, px);
        assert!((got.0 - world.0).abs() < 1e-3 && (got.1 - world.1).abs() < 1e-3);
    }

    #[test]
    fn minimap_world_point_guards_a_zero_scale() {
        // A degenerate map cannot be inverted: dividing by the zero scale would
        // yield an infinity, so the guard answers the world bounds origin
        // instead of poisoning a camera fit with NaN.
        let transform = (10.0, 20.0, 0.0, 0.0, 4.0, 5.0, 0.0, 0.0);
        let (wx, wy) = minimap_world_point(transform, (100.0, 100.0));
        assert_eq!((wx, wy), (10.0, 20.0));
        let (min_x, min_y, max_x, max_y) =
            minimap_world_bounds(transform, (0.0, 0.0), (180.0, 120.0));
        assert!(min_x.is_finite() && max_x.is_finite());
        assert!(max_x > min_x && max_y > min_y, "a positive span survives");
    }

    #[test]
    fn minimap_world_bounds_normalises_reversed_corners() {
        let transform = (0.0, 0.0, 1000.0, 500.0, 14.0, 104.0, 0.172, 0.224);
        // Two corners of the map: (200, 100) and (600, 400) in world units.
        let a = (14.0 + 200.0 * 0.172, 104.0 + 100.0 * 0.224);
        let b = (14.0 + 600.0 * 0.172, 104.0 + 400.0 * 0.224);
        let (min_x, min_y, max_x, max_y) = minimap_world_bounds(transform, a, b);
        assert!((min_x - 200.0).abs() < 1e-2, "min_x: {min_x}");
        assert!((min_y - 100.0).abs() < 1e-2, "min_y: {min_y}");
        assert!((max_x - 600.0).abs() < 1e-2, "max_x: {max_x}");
        assert!((max_y - 400.0).abs() < 1e-2, "max_y: {max_y}");
        // The drag direction does not matter: the swapped corners give the same
        // normalised rect.
        let swapped = minimap_world_bounds(transform, b, a);
        assert!((swapped.0 - min_x).abs() < 1e-3 && (swapped.3 - max_y).abs() < 1e-3);
    }

    #[test]
    fn minimap_world_bounds_widens_a_zero_span() {
        // A click (or a one-pixel marquee) names a zero-span rect; the fit would
        // then divide by the graph's degenerate-span floor and jump to the zoom
        // ceiling, so the bounds keep a small positive span around the point.
        let transform = (0.0, 0.0, 1000.0, 500.0, 14.0, 104.0, 0.172, 0.224);
        let at = (40.0, 60.0);
        let (min_x, min_y, max_x, max_y) = minimap_world_bounds(transform, at, at);
        assert!(max_x > min_x && max_y > min_y, "span: {min_x}..{max_x}");
        let (px, py) = minimap_world_point(transform, at);
        assert!(
            min_x <= px && px <= max_x && min_y <= py && py <= max_y,
            "the point stays inside the kept span"
        );
    }

    #[test]
    fn minimap_empty_scene_is_none() {
        let empty = SceneSpec {
            background: (0, 0, 0),
            nodes: vec![],
            edges: vec![],
            clusters: vec![],
        };
        assert!(minimap_layout(
            &empty,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
        )
        .is_none());
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
    use std::collections::HashSet;
    use std::path::Path;

    use crate::app::LabelStore;
    use crate::app::{OptimizerDiffState, OptimizerState};
    use crate::config::LayoutMode;
    use crate::diff::{ChangedCable, ChangedNode, DiffReport};
    use crate::graph::{Cluster, Graph, GraphEdge, GraphNode, TopologyIssue, TopologySeverity};
    use crate::latency::{EdgeLatency as ModelEdgeLatency, LatencyData, LatencySummary};
    use crate::optimize::CandidateOrdering;
    use crate::patch::{InfluenceSubtree, NodeId};

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
        build_scene_spec(
            app,
            theme(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(0.0, 0.0)),
        )
        .expect("scene present")
    }

    #[test]
    fn scene_requires_graph_and_aligned_positions() {
        assert!(build_scene_spec(
            &App::new(),
            theme(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(0.0, 0.0))
        )
        .is_none());
        let graph = Graph {
            nodes: vec![circuit_node("copy", 0, 0)],
            ..Graph::default()
        };
        let app = scene_app(graph, &[]);
        assert!(build_scene_spec(
            &app,
            theme(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(0.0, 0.0))
        )
        .is_none());
    }

    /// Arm the optimizer-preview diff on `chain_app`'s single `_CLK` edge:
    /// set the current latency and the captured file-order baseline so the
    /// edge's delta has the given direction (optimizer-click-focus).
    fn arm_opt_diff(app: &mut App, before: f32, current: f32) {
        let t = LatencySummary {
            avg: current,
            max: current,
            back_edge_count: 0,
        };
        app.graph.as_mut().unwrap().latency = Some(LatencyData {
            edges: vec![ModelEdgeLatency {
                edge_index: 0,
                latency: current,
                is_back_edge: false,
            }],
            summary: t,
        });
        app.optimizer_diff_state = Some(OptimizerDiffState {
            before_cable: vec!["_CLK".into()],
            before_latency: vec![before],
        });
        app.optimizer = Some(OptimizerState {
            candidates: vec![CandidateOrdering {
                label: "test".into(),
                order: vec![0, 1],
                before: t,
                after: t,
            }],
            cursor: 0,
            previewing: Some(0),
            original_order: vec![0, 1],
            weight: 0.0,
        });
    }

    #[test]
    fn optimizer_diff_colors_edges_and_nodes_by_latency_direction() {
        let t = theme();
        // Current latency rose above the baseline → worse → red on the edge
        // and on its endpoint nodes.
        let mut worse = chain_app();
        arm_opt_diff(&mut worse, 1.0, 5.0);
        let s = spec(&worse);
        assert_eq!(s.edges[0].color, t.rgb(t.graph_edge_diff_before));
        assert!(
            s.nodes
                .iter()
                .all(|n| n.border == t.rgb(t.graph_node_diff_before)),
            "both endpoint nodes mark the worse edge"
        );

        // Current latency fell below the baseline → better → blue.
        let mut better = chain_app();
        arm_opt_diff(&mut better, 5.0, 1.0);
        let s = spec(&better);
        assert_eq!(s.edges[0].color, t.rgb(t.graph_edge_diff_after));
        assert!(
            s.nodes
                .iter()
                .all(|n| n.border == t.rgb(t.graph_node_diff_after)),
            "both endpoint nodes mark the better edge"
        );
    }

    #[test]
    fn optimizer_diff_inactive_without_preview_keeps_kind_color() {
        let t = theme();
        let mut app = chain_app();
        // Same latency delta as the diff test, but no preview armed → the
        // diff must not color the edge (it keeps its cable-kind color).
        app.graph.as_mut().unwrap().latency = Some(LatencyData {
            edges: vec![ModelEdgeLatency {
                edge_index: 0,
                latency: 5.0,
                is_back_edge: false,
            }],
            summary: LatencySummary {
                avg: 5.0,
                max: 5.0,
                back_edge_count: 0,
            },
        });
        app.optimizer_diff_state = Some(OptimizerDiffState {
            before_cable: vec!["_CLK".into()],
            before_latency: vec![1.0],
        });
        // No preview: optimizer is None → diff inactive.
        // Disable the latency ramp so the edge shows its cable-kind color
        // (a high latency would otherwise light the ramp red, which coincides
        // with the diff token and would defeat the `assert_ne`).
        app.latency_coloring = false;
        let s = spec(&app);
        assert_ne!(
            s.edges[0].color,
            t.rgb(t.graph_edge_diff_before),
            "no preview ⇒ no diff color"
        );
    }

    #[test]
    fn scene_reflects_active_settle_positions() {
        // While a settle animation runs the scene must draw the interpolated
        // seed positions, not the solved layout. Build the settled scene, then
        // the same scene with the settle cleared, and assert node 0 moved.
        let mut app = chain_app();
        // Distinct y so both axes contract; the compact seed is then strictly
        // inside the solved layout on both axes.
        app.graph_positions = vec![(10.0, 20.0), (260.0, 90.0)];
        let solved = spec(&app);
        let seed = crate::graph_anim::compact_seed(
            &app.graph_positions,
            crate::graph_anim::SETTLE_SEED_SCALE,
        );
        // Start in the future so progress clamps to 0 and the drawn positions
        // are exactly the seed (deterministic, no wall-clock drift).
        app.graph_settle = Some(crate::graph_anim::GraphSettle::with_started_at(
            seed,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
        ));
        let animated = spec(&app);
        assert_ne!(
            animated.nodes[0].x, solved.nodes[0].x,
            "an active settle shifts the drawn node"
        );
        assert_ne!(animated.nodes[0].y, solved.nodes[0].y);
        app.cancel_graph_settle();
        let restored = spec(&app);
        assert_eq!(restored.nodes[0].x, solved.nodes[0].x);
        assert_eq!(restored.nodes[0].y, solved.nodes[0].y);
    }

    #[test]
    fn scene_maps_positions_identity_with_fallback_camera() {
        let app = chain_app();
        let s = spec(&app);
        let world = crate::layout::node_world_sizes(app.graph.as_ref().unwrap());
        assert_eq!(s.nodes.len(), 2);
        assert_eq!(s.nodes[0].x, 10.0);
        assert_eq!(s.nodes[0].y, 20.0);
        // Fallback camera zoom 1.0: pixel body size is the world size × 1.
        assert_eq!(s.nodes[0].w, world[0].0 * 1.0);
        assert_eq!(s.nodes[0].h, world[0].1 * 1.0);
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
        // Node bodies scale with the zoom, not only the spacing.
        assert_eq!(z.nodes[0].w, identity.nodes[0].w * 2.0);
        assert_eq!(z.nodes[0].h, identity.nodes[0].h * 2.0);
        assert_eq!(
            z.nodes[1].x - z.nodes[0].x,
            (identity.nodes[1].x - identity.nodes[0].x) * 2.0
        );
    }

    #[test]
    fn scene_node_pixel_size_is_world_size_times_zoom() {
        // graph-zoom-node-scaling decision 1: a node's pixel body is its
        // reserved world size multiplied by the camera zoom.
        let world = crate::layout::node_world_sizes(chain_app().graph.as_ref().unwrap());

        let identity = spec(&chain_app());
        assert_eq!(identity.nodes[0].w, world[0].0 * 1.0);
        assert_eq!(identity.nodes[0].h, world[0].1 * 1.0);
        assert_eq!(identity.nodes[0].radius, NODE_RADIUS * 1.0);

        let mut half = chain_app();
        half.graph_camera = Some(GraphCamera {
            zoom: 0.5,
            pan: (0.0, 0.0),
        });
        let h = spec(&half);
        assert_eq!(h.nodes[0].w, world[0].0 * 0.5);
        assert_eq!(h.nodes[0].h, world[0].1 * 0.5);
        assert_eq!(h.nodes[0].radius, NODE_RADIUS * 0.5);

        let mut double = chain_app();
        double.graph_camera = Some(GraphCamera {
            zoom: 2.0,
            pan: (0.0, 0.0),
        });
        let d = spec(&double);
        assert_eq!(d.nodes[0].w, world[0].0 * 2.0);
        assert_eq!(d.nodes[0].h, world[0].1 * 2.0);
        assert_eq!(d.nodes[0].radius, NODE_RADIUS * 2.0);
    }

    /// A graph whose first node is wider than the sections lane cap and whose
    /// second is narrower, for the mode-scoped width clamp (3.10).
    fn wide_then_narrow_app() -> App {
        // 40 chars × NODE_ESTIMATE_CHAR_WIDTH (8) + padding = 336 > cap (240).
        let wide = "w".repeat(40);
        let graph = Graph {
            nodes: vec![circuit_node(&wide, 0, 0), circuit_node("copy", 0, 1)],
            ..Graph::default()
        };
        scene_app(graph, &[(0.0, 0.0), (400.0, 0.0)])
    }

    #[test]
    fn scene_sections_caps_wide_node_drawn_and_hit_width() {
        let cap = crate::layout::SECTIONS_LANE_MAX_NODE_W;
        let mut app = wide_then_narrow_app();
        app.layout_mode = LayoutMode::Sections;
        let raw = crate::layout::node_world_sizes(app.graph.as_ref().unwrap());
        assert!(raw[0].0 > cap, "fixture guard: raw {} > cap", raw[0].0);

        let s = spec(&app); // fallback camera: zoom 1, pan 0
        assert_eq!(s.nodes[0].w, cap, "drawn width is clamped to the lane cap");
        assert_eq!(s.nodes[0].h, raw[0].1, "height is not clamped");

        // Hit geometry follows the drawn frame: inside the cap hits, past it
        // misses — the spec-space pointer path must not hover a wider region.
        let (nx, ny) = (s.nodes[0].x, s.nodes[0].y);
        assert_eq!(node_at(&s, nx + cap - 1.0, ny + 1.0), Some(0));
        assert_eq!(node_at(&s, nx + cap + 1.0, ny + 1.0), None);
        assert!(nodes_in_rect(&s, (nx + cap + 1.0, ny, 1.0, 1.0)).is_empty());
    }

    #[test]
    fn scene_sections_cap_is_world_space_not_pixel_space() {
        // The cap applies to the world width before projection, so a zoomed
        // scene scales the capped body (cap × zoom) instead of the estimate.
        let cap = crate::layout::SECTIONS_LANE_MAX_NODE_W;
        let mut app = wide_then_narrow_app();
        app.layout_mode = LayoutMode::Sections;
        app.graph_camera = Some(GraphCamera {
            zoom: 2.0,
            pan: (0.0, 0.0),
        });
        let s = spec(&app);
        assert_eq!(s.nodes[0].w, cap * 2.0);
    }

    #[test]
    fn scene_sections_leaves_a_narrower_node_unclamped() {
        let cap = crate::layout::SECTIONS_LANE_MAX_NODE_W;
        let mut app = wide_then_narrow_app();
        app.layout_mode = LayoutMode::Sections;
        let raw = crate::layout::node_world_sizes(app.graph.as_ref().unwrap());
        assert!(raw[1].0 < cap, "fixture guard: narrow {} < cap", raw[1].0);

        let s = spec(&app);
        assert_eq!(s.nodes[1].w, raw[1].0);
        let (nx, ny) = (s.nodes[1].x, s.nodes[1].y);
        assert_eq!(node_at(&s, nx + raw[1].0 - 1.0, ny + 1.0), Some(1));
        assert_eq!(node_at(&s, nx + raw[1].0 + 1.0, ny + 1.0), None);
    }

    #[test]
    fn scene_column_and_force_modes_keep_the_raw_world_width() {
        // ADR 40 guard: the sections cap is mode-scoped, so column and force
        // keep drawing the layout's own per-node width estimate.
        let cap = crate::layout::SECTIONS_LANE_MAX_NODE_W;
        for mode in [LayoutMode::Column, LayoutMode::Force] {
            let mut app = wide_then_narrow_app();
            app.layout_mode = mode;
            let raw = crate::layout::node_world_sizes(app.graph.as_ref().unwrap());
            assert!(raw[0].0 > cap, "fixture guard: raw {} > cap", raw[0].0);
            let s = spec(&app);
            assert_eq!(s.nodes[0].w, raw[0].0, "{mode:?} keeps the raw width");
            assert_eq!(s.nodes[1].w, raw[1].0, "{mode:?} keeps the raw width");
        }
    }

    #[test]
    fn scene_sections_capped_width_still_frames_through_the_camera_path() {
        // The capped body stays inside the extent the fit reserved (the raw,
        // uncapped max world width), so the fitted camera still frames it.
        let mut app = wide_then_narrow_app();
        app.layout_mode = LayoutMode::Sections;
        let canvas = (1280.0, 800.0);
        let (fit_w, fit_h) = app.graph_fit_node_world();
        app.graph_camera = Some(graph_window_fit_camera(
            &app.graph_positions,
            (fit_w, fit_h),
            canvas,
        ));
        let s = spec(&app);
        let zoom = spec_zoom(&s);
        assert_eq!(s.nodes.len(), 2);
        for (i, n) in s.nodes.iter().enumerate() {
            assert!(
                n.x.is_finite() && n.y.is_finite() && n.w.is_finite() && n.h.is_finite(),
                "node {i} geometry finite"
            );
            assert!(n.w > 0.0 && n.h > 0.0, "node {i} has a body");
            assert!(n.w <= fit_w * zoom + 1e-3, "node {i} within the fit width");
            assert!(n.h <= fit_h * zoom + 1e-3, "node {i} within the fit height");
            assert!(
                n.x >= -0.5 && n.x + n.w <= canvas.0 + 0.5,
                "node {i} on canvas: {}..{}",
                n.x,
                n.x + n.w
            );
            assert!(
                n.y >= -0.5 && n.y + n.h <= canvas.1 + 0.5,
                "node {i} on canvas: {}..{}",
                n.y,
                n.y + n.h
            );
        }
    }

    #[test]
    fn scene_geometry_keeps_minimum_clamps_at_low_zoom() {
        // graph-zoom-node-scaling decision 2: at the zoom floor the scaled
        // chrome bottoms out on its minimum instead of vanishing.
        let graph = Graph {
            nodes: vec![circuit_node("copy", 0, 0), circuit_node("mixer", 0, 1)],
            clusters: vec![Cluster {
                title: "Core".into(),
                section_range: 0..2,
            }],
            ..Graph::default()
        };
        let mut app = scene_app(graph, &[(0.0, 0.0), (260.0, 0.0)]);
        app.graph_camera = Some(GraphCamera {
            zoom: 0.0078,
            pan: (0.0, 0.0),
        });
        app.hovered_graph_node = Some(0);
        let s = spec(&app);

        for n in &s.nodes {
            assert!(
                n.w > 0.0 && n.h > 0.0,
                "no zero node body: {:?}",
                (n.w, n.h)
            );
            assert!(n.w.is_finite() && n.h.is_finite() && n.radius.is_finite());
            assert!(n.border_width >= 0.5, "border {} >= 0.5", n.border_width);
        }
        // The highlighted node keeps the 3.0-vs-1.0 distinction, floored at 1.0.
        assert!(s.nodes[0].border_width >= 1.0);
        for e in &s.edges {
            assert!(e.width.is_finite() && e.width >= 0.5, "edge {}", e.width);
        }
        let c = &s.clusters[0];
        let min_x = s.nodes.iter().map(|n| n.x).fold(f32::INFINITY, f32::min);
        assert!(
            min_x - c.x >= 2.0 - 1e-3,
            "cluster pad {} >= 2.0",
            min_x - c.x
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
    fn scene_labels_render_guessed_label_without_store_entry() {
        // label-guess-and-screen-scale 1.3: with no stored label, the node
        // title comes from the `guessed_labels` cache. The cache entry is
        // primed directly (no walk runs here), proving the scene build reads
        // the cache instead of walking the graph per frame.
        let mut app = chain_app();
        app.guessed_labels
            .insert(NodeId::circuit("osc", 0), "Guessed Osc".into());
        let s = spec(&app);
        assert_eq!(s.nodes[1].label, "Guessed Osc");
        // The unlabeled, unguessed node keeps its raw circuit name.
        assert_eq!(s.nodes[0].label, "clocktool");
    }

    #[test]
    fn scene_labels_stored_label_beats_guessed_label() {
        // Chain order: a stored label always wins over a cached guess.
        let mut app = chain_app();
        app.current_patch_path = Some(Path::new("fixture.ini").to_path_buf());
        app.label_store
            .patch_labels_mut(Path::new("fixture.ini"))
            .circuits
            .insert(LabelStore::encode_node_id("osc", 0), "Stored Osc".into());
        app.guessed_labels
            .insert(NodeId::circuit("osc", 0), "Guessed Osc".into());
        let s = spec(&app);
        assert_eq!(s.nodes[1].label, "Stored Osc");
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
    fn scene_influence_highlights_influenced_and_dims_rest() {
        // Influence covers the whole 2-node `_CLK` chain: both nodes and the
        // edge render highlighted/bold.
        let mut influenced = HashSet::new();
        influenced.insert(NodeId::circuit("clocktool", 0));
        influenced.insert(NodeId::circuit("osc", 0));
        let mut edges = HashSet::new();
        edges.insert("_CLK".to_string());
        let mut app = chain_app();
        app.influence = Some(InfluenceSubtree {
            influenced_nodes: influenced,
            influenced_edges: edges,
        });
        let s = spec(&app);
        assert_eq!(s.nodes[0].border_width, 3.0);
        assert_eq!(s.nodes[0].border, theme().rgb(theme().graph_node_highlight));
        assert_eq!(s.nodes[1].border, theme().rgb(theme().graph_node_highlight));
        assert_eq!(s.edges[0].width, EDGE_WIDTH + 1.0);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_highlight));
        assert!(!s.edges[0].dim);

        // A node outside the influence set dims while influence is active.
        let mut influenced = HashSet::new();
        influenced.insert(NodeId::circuit("clocktool", 0));
        let mut app = chain_app();
        app.influence = Some(InfluenceSubtree {
            influenced_nodes: influenced,
            influenced_edges: HashSet::new(),
        });
        let s = spec(&app);
        assert_eq!(s.nodes[0].border, theme().rgb(theme().graph_node_highlight));
        assert_eq!(s.nodes[1].border, theme().rgb(theme().graph_node_dim));
        assert_eq!(s.nodes[1].border_width, 1.0);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_dim));
        assert!(s.edges[0].dim);
    }

    #[test]
    fn scene_element_focus_highlights_register_edge_and_controller_node() {
        // droid_tui-8ia: the element-focus set (`_REG:<token>` + both endpoints,
        // the controller node and the sink circuit) drives the same highlight
        // the influence walk uses, so a clicked module-UI cell lights its own
        // graph entity even when the walk alone would not cover the controller.
        let controller = GraphNode {
            id: NodeId::Controller(String::from("p2b8"), 1),
            kind: NodeKind::Controller,
            circuit: String::from("p2b8"),
            instance_index: 1,
            section_index: 0,
        };
        let graph = Graph {
            nodes: vec![circuit_node("button", 0, 1), controller],
            edges: vec![GraphEdge {
                cable: "_REG:B1.1".into(),
                source: NodeId::Controller(String::from("p2b8"), 1),
                sink: NodeId::circuit("button", 0),
            }],
            ..Graph::default()
        };
        let mut app = scene_app(graph, &[(10.0, 20.0), (260.0, 20.0)]);
        let mut nodes = HashSet::new();
        nodes.insert(NodeId::circuit("button", 0));
        nodes.insert(NodeId::Controller(String::from("p2b8"), 1));
        let mut edges = HashSet::new();
        edges.insert(String::from("_REG:B1.1"));
        app.influence = Some(InfluenceSubtree {
            influenced_nodes: nodes,
            influenced_edges: edges,
        });
        let s = spec(&app);
        assert_eq!(s.nodes[0].border, theme().rgb(theme().graph_node_highlight));
        assert_eq!(s.nodes[1].border, theme().rgb(theme().graph_node_highlight));
        assert_eq!(s.nodes[1].border_width, 3.0);
        assert_eq!(s.edges[0].color, theme().rgb(theme().graph_edge_highlight));
        assert_eq!(s.edges[0].width, EDGE_WIDTH + 1.0);
        assert!(!s.edges[0].dim);
    }

    #[test]
    fn scene_influence_hover_still_beats_influence_styling() {
        // Hover is interaction feedback: it outranks the influence dim so the
        // user sees the pointer target even when it is not influenced.
        let mut influenced = HashSet::new();
        influenced.insert(NodeId::circuit("clocktool", 0));
        let mut app = chain_app();
        app.influence = Some(InfluenceSubtree {
            influenced_nodes: influenced,
            influenced_edges: HashSet::new(),
        });
        app.hovered_graph_node = Some(1);
        let s = spec(&app);
        assert_eq!(s.nodes[1].border, theme().rgb(theme().graph_node_highlight));
        assert_eq!(s.nodes[1].border_width, 3.0);
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
            (
                f32::midpoint(e.start.0, e.end.0),
                f32::midpoint(e.start.1, e.end.1)
            )
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
        // Coincident nodes: the edge spans the source node's body width.
        assert_eq!(s.edges[0].start.0 - s.edges[0].end.0, s.nodes[0].w);
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
        let world = crate::layout::node_world_sizes(&graph);
        // Fallback camera zoom 1.0: cluster pad is the base CLUSTER_PAD.
        let s = spec(&scene_app(graph, &[(100.0, 100.0), (260.0, 60.0)]));
        assert_eq!(s.clusters.len(), 1);
        let c = &s.clusters[0];
        assert_eq!(c.title, "Core");
        assert_eq!(c.member_indices, vec![0, 1]);
        assert_eq!(c.x, 100.0 - CLUSTER_PAD);
        assert_eq!(c.y, 60.0 - CLUSTER_PAD);
        assert_eq!(c.x + c.w, 260.0 + world[1].0 * 1.0 + CLUSTER_PAD);
        assert_eq!(c.border, theme().rgb(theme().graph_cluster_border));
        assert_eq!(c.title_color, theme().rgb(theme().graph_cluster_title));
    }

    /// Four nodes in two banner groups (nodes 0..2 and 2..4): the fixture that
    /// gives a sections graph clusters the solver anchors on, and that the
    /// suppression tests below share.
    fn two_group_app() -> App {
        crate::test_support::two_group_app()
    }

    #[test]
    fn scene_sections_mode_draws_no_cluster_rectangles() {
        // Spec "No cluster boxes drawn in sections mode": the clustering itself
        // visualizes sections, so no banner-group enclosing rectangle is built.
        let mut app = two_group_app();
        app.layout_mode = LayoutMode::Sections;
        assert_eq!(
            app.graph.as_ref().unwrap().clusters.len(),
            2,
            "fixture guard: two banner groups to suppress"
        );
        let s = spec(&app);
        assert!(
            s.clusters.is_empty(),
            "sections mode builds no cluster rectangles: {:?}",
            s.clusters
        );
        // A box that is not drawn is not drawn at all: the scene carries no
        // cluster rectangles in sections mode, and the pane publishes no graph
        // hit geometry either way (hit-testing is world space, ADR 40).
    }

    #[test]
    fn scene_sections_multi_group_keeps_the_cluster_data() {
        // Only the rendering is suppressed: the model keeps its banner groups
        // so the solver's per-(group, row) anchors stay available.
        let mut app = two_group_app();
        app.layout_mode = LayoutMode::Sections;
        assert!(spec(&app).clusters.is_empty());
        let clusters = &app.graph.as_ref().unwrap().clusters;
        assert_eq!(clusters.len(), 2, "the cluster data survives sections mode");
        assert_eq!(clusters[0].title, "Controls");
        assert_eq!(clusters[0].section_range, 0..2);
        assert_eq!(clusters[1].title, "Audio");
        assert_eq!(clusters[1].section_range, 2..4);
    }

    #[test]
    fn scene_column_and_force_modes_keep_cluster_rectangles_unchanged() {
        // Regression guard: the sections suppression is mode-scoped, so column
        // and force keep drawing the very same enclosing rectangles.
        let mut column_app = two_group_app();
        column_app.layout_mode = LayoutMode::Column;
        let column = spec(&column_app);

        let mut force_app = two_group_app();
        force_app.layout_mode = LayoutMode::Force;
        let force = spec(&force_app);

        assert_eq!(column.clusters.len(), 2, "column keeps both boxes");
        assert_eq!(force.clusters.len(), 2, "force keeps both boxes");
        assert_eq!(
            column.clusters, force.clusters,
            "both modes draw identical cluster geometry"
        );

        // The numbers are still the padded member union the cluster-bounds test
        // above pins, so neither mode's cluster geometry shifted.
        let world = crate::layout::node_world_sizes(column_app.graph.as_ref().unwrap());
        let first = &column.clusters[0];
        assert_eq!(first.title, "Controls");
        assert_eq!(first.member_indices, vec![0, 1]);
        assert_eq!(first.x, -CLUSTER_PAD);
        assert_eq!(first.y, -CLUSTER_PAD);
        assert_eq!(first.x + first.w, 260.0 + world[1].0 + CLUSTER_PAD);
        assert_eq!(first.border, theme().rgb(theme().graph_cluster_border));
        assert_eq!(first.title_color, theme().rgb(theme().graph_cluster_title));
        let second = &column.clusters[1];
        assert_eq!(second.title, "Audio");
        assert_eq!(second.member_indices, vec![2, 3]);
        assert_eq!(second.x, 520.0 - CLUSTER_PAD);
        assert_eq!(second.x + second.w, 780.0 + world[3].0 + CLUSTER_PAD);
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
/// (canvas smaller than the fitted scene, so the scene overflows it), and zoomed
/// (camera zoom scales node spacing). A camera/scene regression that silently
/// repaints the window black or pushes content off-canvas now fails the suite
/// instead of surfacing only on a live screen (bead droid_tui-69r).
#[cfg(test)]
mod window_paint_tests {
    use super::*;
    use crate::config::LayoutMode;
    use crate::diff::ChangedNode;
    use crate::graph::{Graph, GraphEdge, GraphNode, TopologyIssue, TopologySeverity};
    use crate::patch::{InfluenceSubtree, NodeId};

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
        /// The `FontId` each label was painted with, parallel to `labels`.
        label_fonts: Vec<egui::FontId>,
        /// The `Color32` each label was painted with, parallel to `labels`: the
        /// emitted `Shape::Text` colour, which is what a reader actually sees.
        label_colors: Vec<egui::Color32>,
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
        paint_ui(raw_input, |ui| paint_scene(ui, canvas, scene, &[]))
    }

    /// One painted frame's shapes, for any paint entry point: `body` receives the
    /// `Ui` the scene is painted into (the group peek needs `paint_scene_in` with
    /// an `App`, which the compat wrapper above deliberately does not take).
    fn paint_ui(raw_input: egui::RawInput, body: impl FnMut(&mut egui::Ui)) -> PaintOutput {
        let ctx = egui::Context::default();
        let mut full_output = ctx.run_ui(raw_input, body);
        let mut labels = Vec::new();
        let mut label_fonts = Vec::new();
        let mut label_colors = Vec::new();
        let mut rects = Vec::new();
        let mut circles = 0usize;
        let mut beziers = 0usize;
        let mut polygons = 0usize;
        for cs in &full_output.shapes {
            match &cs.shape {
                egui::epaint::Shape::Text(t) => {
                    labels.push(t.galley.text().to_string());
                    // The painter lays every label out as a single-section
                    // `LayoutJob`, so the section format carries the FontId and
                    // the colour the galley was laid out with.
                    let format = t.galley.job.sections.first().map(|s| &s.format);
                    label_fonts.push(format.map(|f| f.font_id.clone()).unwrap_or_default());
                    label_colors.push(format.map_or(egui::Color32::TRANSPARENT, |f| f.color));
                }
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
            label_fonts,
            label_colors,
            rects,
            circles,
            beziers,
            polygons,
        }
    }

    /// Left-edge x of each node body fill: for every scene node, the filled
    /// rect whose geometry matches that node's frame. The full-canvas
    /// background, minimap panel, and port circles are a different size or
    /// shape, and node borders draw with a transparent fill, so matching the
    /// scene frames isolates the bodies at any zoom (node size is world × zoom
    /// since graph-zoom-node-scaling).
    // ── graph-zoom-node-scaling: level of detail (task 2.2) ─────────────────

    #[test]
    fn fit_label_fixed_font_ellipsizes_wider_titles() {
        // A title wider than its frame is ellipsized to the characters that fit
        // at the fixed NODE_LABEL_SIZE font — the font never shrinks to fit
        // (spec "Level of detail at extreme zoom").
        let (text, font) = fit_label("resonant_filter", 100.0).expect("fits");
        assert_eq!(font, NODE_LABEL_SIZE);
        assert!(text.ends_with('\u{2026}'), "ellipsized: {text:?}");
        assert!(
            text.chars().count() as f32 * MONO_ADVANCE * font <= 100.0 + 1e-3,
            "ellipsized text fits the frame width: {text:?} at {font} pt"
        );

        // A title that fits is returned unchanged at the same fixed font.
        let (text, font) = fit_label("osc", 400.0).expect("fits");
        assert_eq!(text, "osc");
        assert_eq!(font, NODE_LABEL_SIZE);
    }

    #[test]
    fn fit_label_hidden_below_one_character_width() {
        // A frame narrower than one character of the fixed font paints no
        // label; nor does a zero width or an empty label.
        let char_w = MONO_ADVANCE * NODE_LABEL_SIZE;
        assert!(fit_label("osc", char_w * 0.9).is_none());
        assert!(fit_label("osc", 0.0).is_none());
        assert!(fit_label("", 400.0).is_none());
        // Exactly one character of budget: the ellipsis alone fills it.
        let (text, font) = fit_label("osc", char_w).expect("one character shows");
        assert_eq!(text, "\u{2026}");
        assert_eq!(font, NODE_LABEL_SIZE);
    }

    /// Spec "Label size independent of zoom": every painted node label uses the
    /// same fixed font at camera zoom 0.125, 1.0, and 2.0 — node frames scale
    /// with the camera, the label font does not.
    #[test]
    fn node_label_font_size_is_identical_across_zoom() {
        for &zoom in &[0.125_f32, 1.0, 2.0] {
            let mut app = chain_app();
            app.graph_camera = Some(GraphCamera {
                zoom,
                pan: (0.0, 0.0),
            });
            let scene = build_scene_spec(
                &app,
                theme(),
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
            )
            .expect("scene present");
            let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
            assert!(
                !out.labels.is_empty(),
                "a label paints at zoom {zoom}: {:?}",
                out.labels
            );
            assert_eq!(out.labels.len(), out.label_fonts.len());
            for (label, font) in out.labels.iter().zip(&out.label_fonts) {
                assert_eq!(
                    *font,
                    egui::FontId::monospace(NODE_LABEL_SIZE),
                    "zoom {zoom}: label {label:?} must paint at the fixed font"
                );
            }
        }
    }

    /// Spec "Long label fits its frame": a title wider than its frame is
    /// ellipsized to the frame, and the font stays at the fixed size.
    #[test]
    fn node_label_ellipsizes_at_medium_frame() {
        let scene = SceneSpec {
            background: (10, 10, 10),
            nodes: vec![NodeSpec {
                x: 10.0,
                y: 10.0,
                w: 100.0,
                h: 40.0,
                radius: 1.0,
                fill: (20, 20, 20),
                border: (200, 200, 200),
                border_width: 1.0,
                label: "resonant_filter".into(),
                label_color: (255, 255, 255),
                circuit: "resonant_filter".into(),
                instance_index: 0,
                input_port: false,
                output_port: false,
            }],
            edges: vec![],
            clusters: vec![],
        };
        let out = paint(Some(&scene), egui::vec2(400.0, 200.0));
        assert_eq!(out.labels.len(), 1, "one node label: {:?}", out.labels);
        let text = &out.labels[0];
        assert!(
            text.ends_with('\u{2026}'),
            "ellipsized rather than overflowing: {text:?}"
        );
        assert_eq!(
            out.label_fonts[0],
            egui::FontId::monospace(NODE_LABEL_SIZE),
            "fitting never changes the font size"
        );
        assert!(
            text.chars().count() as f32 * MONO_ADVANCE * NODE_LABEL_SIZE <= 100.0 + 1e-3,
            "painted text fits the frame: {text:?}"
        );
    }

    /// Spec "Labels hidden when frames are too small": a frame narrower than
    /// one character of the fixed font renders with no title text.
    #[test]
    fn node_label_hidden_at_one_character_frame() {
        let scene = SceneSpec {
            background: (10, 10, 10),
            nodes: vec![NodeSpec {
                x: 10.0,
                y: 10.0,
                w: MONO_ADVANCE * NODE_LABEL_SIZE * 0.9,
                h: 40.0,
                radius: 1.0,
                fill: (20, 20, 20),
                border: (200, 200, 200),
                border_width: 1.0,
                label: "osc".into(),
                label_color: (255, 255, 255),
                circuit: "osc".into(),
                instance_index: 0,
                input_port: false,
                output_port: false,
            }],
            edges: vec![],
            clusters: vec![],
        };
        let out = paint(Some(&scene), egui::vec2(400.0, 200.0));
        assert!(
            out.labels.is_empty(),
            "no label below one character: {:?}",
            out.labels
        );
    }

    fn node_origins(out: &PaintOutput, scene: &SceneSpec) -> Vec<f32> {
        let mut xs: Vec<f32> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                out.rects.iter().find(|r| {
                    (r.rect.left() - n.x).abs() < 0.5
                        && (r.rect.top() - n.y).abs() < 0.5
                        && (r.rect.width() - n.w).abs() < 0.5
                        && (r.rect.height() - n.h).abs() < 0.5
                        && r.fill != egui::Color32::TRANSPARENT
                })
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
        app.graph_camera = Some(graph_window_fit_camera(
            &app.graph_positions,
            app.graph_fit_node_world(),
            (1280.0, 800.0),
        ));
        let scene = build_scene_spec(
            &app,
            theme(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
        )
        .expect("scene present");
        let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));

        // label-guess-and-screen-scale design decision 4: labels paint at a
        // fixed font and are ellipsized only when wider than the frame, so a
        // title may paint "o…" at the fit zoom. Assert the painted labels carry
        // the identity prefixes rather than exact full names.
        assert!(
            out.labels.iter().any(|l| l.starts_with("clockto")),
            "clocktool label: {:?}",
            out.labels
        );
        assert!(
            out.labels.iter().any(|l| l.starts_with('o')),
            "osc label: {:?}",
            out.labels
        );
        assert_eq!(out.beziers, 1, "one cable edge");
        assert_eq!(out.polygons, 1, "one direction arrow");
        assert_eq!(out.circles, 2, "output + input port markers");

        // A label wider than its frame at the fixed font may be ellipsized, so
        // the label assertions above match prefixes rather than full names.

        let origins = node_origins(&out, &scene);
        assert_eq!(origins.len(), 2, "both node bodies drawn: {origins:?}");
        // The fixed-frame fit camera (`graph_window_fit_camera`) still reserves
        // the old pixel frame, so with per-node world widths the trailing node
        // may run past the pane; task 2.3 migrates the fit to frame whole
        // bodies. Until then the regression is that every frame is on canvas.
        for (i, n) in scene.nodes.iter().enumerate() {
            assert!(
                n.x >= -0.5 && n.x < 1280.5 && n.x + n.w > -0.5,
                "node {i} frame visible on canvas: {}..{}",
                n.x,
                n.x + n.w
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
        let scene = build_scene_spec(
            &app,
            theme(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
        )
        .expect("scene present");
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
        // label-guess-and-screen-scale decision 4: the node label keeps the
        // fixed font and may be ellipsized to its frame, so the identity check
        // matches on the label prefix rather than the full name.
        assert!(
            out.labels.iter().any(|l| l.starts_with("clockto")),
            "node label painted (possibly ellipsized): {:?}",
            out.labels
        );
        assert!(
            out.labels
                .iter()
                .any(|l| l.contains("clocktool") && l != "clockto…"),
            "tooltip carries the full circuit name: {:?}",
            out.labels
        );
        assert!(
            hits >= 1,
            "clocktool text painted: {hits}: {:?}",
            out.labels
        );
        // The tooltip card is a dialog backdrop: no rect in this frame may be
        // partially transparent (the card was a 235-alpha wash before). The
        // minimap's viewport indicator is the one deliberate wash — chrome over
        // the map, drawn on every scene since the map is always on — so it is
        // excluded by geometry.
        let panel = map_panel_rect(&scene, canvas);
        let translucent: Vec<_> = out
            .rects
            .iter()
            .filter(|r| r.fill.a() != 255 && r.fill.a() != 0)
            .filter(|r| panel.is_none_or(|p| !p.contains_rect(r.rect)))
            .map(|r| (r.rect, r.fill))
            .collect();
        assert!(
            translucent.is_empty(),
            "tooltip backdrop must be opaque: {translucent:?}"
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
        // A mid-sized canvas smaller than the fitted scene overflows: seed
        // the design-viewport fit and paint into a smaller canvas, modelling
        // the zoomed-in state, whose minimap sits bottom-left.
        let mut app = chain_app();
        app.graph_camera = Some(graph_window_fit_camera(
            &app.graph_positions,
            app.graph_fit_node_world(),
            (1280.0, 800.0),
        ));
        let scene = build_scene_spec(
            &app,
            theme(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)),
        )
        .expect("scene present");
        let out = paint(Some(&scene), egui::vec2(400.0, 300.0));

        assert!(
            out.labels.iter().any(|l| l.contains("clocktool")),
            "content still painted"
        );
        let panel = egui::Rect::from_min_size(egui::pos2(10.0, 170.0), egui::vec2(180.0, 120.0));
        let panel_fill = out.rects.iter().find(|r| r.rect == panel).map_or_else(
            || {
                panic!(
                    "minimap panel drawn bottom-left: {:?}",
                    out.rects.iter().map(|r| r.rect).collect::<Vec<_>>()
                )
            },
            |r| r.fill,
        );
        assert_ne!(panel_fill, egui::Color32::TRANSPARENT);
        assert_eq!(panel_fill.a(), 255, "minimap panel must be opaque");
    }

    #[test]
    fn window_paint_zoomed_scales_node_spacing() {
        // Zooming the camera doubles the node spacing on the painted canvas;
        // the window reflects camera zoom rather than a fixed spec layout.
        let identity = {
            let app = chain_app();
            let scene = build_scene_spec(
                &app,
                theme(),
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
            )
            .expect("scene present");
            let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
            node_origins(&out, &scene)
        };
        let mut zoomed_app = chain_app();
        zoomed_app.graph_camera = Some(GraphCamera {
            zoom: 2.0,
            pan: (0.0, 0.0),
        });
        let zoomed = {
            let scene = build_scene_spec(
                &zoomed_app,
                theme(),
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
            )
            .expect("scene present");
            let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
            node_origins(&out, &scene)
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

    /// Port markers drop below the legibility threshold (graph-zoom-node-scaling
    /// design decision 3); node labels follow their own rule — painted at the
    /// fixed font whenever the frame is at least one character wide, hidden
    /// below it (label-guess-and-screen-scale design decision 4 / spec "Labels
    /// hidden when frames are too small").
    #[test]
    fn window_paint_omits_ports_below_threshold_and_labels_below_one_char() {
        // Bare NodeSpec below the port threshold with both ports set: the wide
        // frame still carries its label — labels no longer gate on height.
        let small = NodeSpec {
            x: 0.0,
            y: 0.0,
            w: 300.0,
            h: LOD_MIN_NODE_H - 1.0,
            radius: 1.0,
            fill: (20, 20, 20),
            border: (200, 200, 200),
            border_width: 1.0,
            label: "visible_title".into(),
            label_color: (255, 255, 255),
            circuit: "osc".into(),
            instance_index: 0,
            input_port: true,
            output_port: true,
        };
        let scene = SceneSpec {
            background: (10, 10, 10),
            nodes: vec![small],
            edges: vec![],
            clusters: vec![],
        };
        let out = paint(Some(&scene), egui::vec2(400.0, 200.0));
        assert!(
            out.labels.iter().any(|l| l.contains("visible_title")),
            "labels gate on width, not height: {:?}",
            out.labels
        );
        assert_eq!(out.circles, 0, "no port markers below the threshold");

        // Narrower than one character of the fixed font: no label at all,
        // even with a frame height above the port threshold.
        let mut narrow = scene.clone();
        narrow.nodes[0].w = MONO_ADVANCE * NODE_LABEL_SIZE * 0.9;
        narrow.nodes[0].h = LOD_MIN_NODE_H * 2.0;
        let out = paint(Some(&narrow), egui::vec2(400.0, 200.0));
        assert!(
            out.labels.is_empty(),
            "no label below one character: {:?}",
            out.labels
        );

        // The same node above the port threshold with a wide frame paints both.
        let mut large = scene.clone();
        large.nodes[0].h = LOD_MIN_NODE_H * 2.0;
        let out = paint(Some(&large), egui::vec2(400.0, 200.0));
        assert!(out.labels.iter().any(|l| l.contains("visible_title")));
        assert_eq!(out.circles, 2, "both ports above the threshold");
    }

    /// Minimap node dots keep a ≥ 2 px floor even for a tiny scene
    /// (graph-zoom-node-scaling task 2.2).
    #[test]
    fn window_paint_minimap_dots_have_two_px_floor() {
        // A spread of tiny node frames that overflows a small canvas, so the
        // minimap draws and every dot shrinks below the floor.
        let node = |x: f32| NodeSpec {
            x,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            radius: 0.0,
            fill: (20, 20, 20),
            border: (200, 200, 200),
            border_width: 1.0,
            label: String::new(),
            label_color: (255, 255, 255),
            circuit: String::new(),
            instance_index: 0,
            input_port: false,
            output_port: false,
        };
        let scene = SceneSpec {
            background: (10, 10, 10),
            nodes: vec![node(0.0), node(500.0), node(1000.0)],
            edges: vec![],
            clusters: vec![],
        };
        let canvas = egui::vec2(200.0, 150.0);
        let minimap = minimap_layout(&scene, egui::Rect::from_min_size(egui::Pos2::ZERO, canvas))
            .expect("scene overflows the canvas");
        let out = paint(Some(&scene), canvas);

        // The mapped node rects are sub-pixel; every painted dot must still be
        // at least 2 px on both axes.
        for &(_, _, w, _) in &minimap.nodes {
            assert!(w < MINIMAP_DOT_MIN, "dot width is tiny: {w}");
        }
        // Node fills draw before the dots, so compare against the raw minimap
        // mapping rather than a scene-frame match: the dots are the rects that
        // grew to the floor.
        for &(_, _, w, _) in &minimap.nodes {
            let painted = out.rects.iter().any(|r| {
                (r.rect.width() - w.max(MINIMAP_DOT_MIN)).abs() < 0.5
                    && r.fill != egui::Color32::TRANSPARENT
            });
            assert!(
                painted,
                "node dot {w} px wide painted at the {MINIMAP_DOT_MIN} px floor"
            );
        }
    }

    /// The minimap region-zoom marquee paints exactly while `App::minimap_drag`
    /// is set, at the drag's window-space corners: a translucent accent fill plus
    /// a 1 px accent stroke.
    #[test]
    fn window_paint_draws_the_minimap_marquee_only_while_dragging() {
        let node = |x: f32| NodeSpec {
            x,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            radius: 0.0,
            fill: (20, 20, 20),
            border: (200, 200, 200),
            border_width: 1.0,
            label: String::new(),
            label_color: (255, 255, 255),
            circuit: String::new(),
            instance_index: 0,
            input_port: false,
            output_port: false,
        };
        let scene = SceneSpec {
            background: (10, 10, 10),
            nodes: vec![node(0.0), node(500.0), node(1000.0)],
            edges: vec![],
            clusters: vec![],
        };
        let canvas = egui::vec2(200.0, 150.0);
        let raw_input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
            ..Default::default()
        };
        // The marquee spans (30, 40) → (120, 90) in window points, inside the
        // panel the canvas places at (10, 20, 180, 120).
        let drag = (30.0, 40.0, 120.0, 90.0);
        let marquee_painted = |out: &PaintOutput| {
            out.rects.iter().any(|r| {
                (r.rect.min.x - drag.0).abs() < 0.5
                    && (r.rect.min.y - drag.1).abs() < 0.5
                    && (r.rect.width() - (drag.2 - drag.0)).abs() < 0.5
                    && (r.rect.height() - (drag.3 - drag.1)).abs() < 0.5
                    // The fill and the 1 px stroke are two shapes at the same
                    // rect; either one proves the marquee is drawn.
                    && (r.fill.a() > 0 || r.stroke.width > 0.0)
            })
        };

        let mut app = App::new();
        app.minimap_drag = Some(drag);
        let dragging = paint_ui(raw_input(), |ui| {
            paint_scene_in(
                ui,
                egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
                Some(&scene),
                &[],
                Some(&app),
            );
        });
        assert!(
            marquee_painted(&dragging),
            "the marquee is drawn while the drag is live"
        );

        let idle = App::new();
        let released = paint_ui(raw_input(), |ui| {
            paint_scene_in(
                ui,
                egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
                Some(&scene),
                &[],
                Some(&idle),
            );
        });
        assert!(!marquee_painted(&released), "no marquee without a drag");
    }

    /// Arrow dimensions shrink with zoom, floored so the direction stays
    /// visible (graph-zoom-node-scaling design decision 2).
    #[test]
    fn paint_arrow_shrinks_with_zoom() {
        // The arrow triangle's apex is the edge end; its base is `arrow_len`
        // back along the tangent. A horizontal edge makes the base offset the
        // triangle's leftmost x, so the painted polygon width measures it.
        let edge = EdgeSpec {
            start: (0.0, 0.0),
            end: (200.0, 0.0),
            ctrl: (100.0, 0.0),
            color: (0, 255, 0),
            width: 2.0,
            kind: CableKind::Audio,
            error: false,
            dim: false,
            diff: None,
            latency: None,
        };
        let scene = SceneSpec {
            background: (10, 10, 10),
            nodes: vec![],
            edges: vec![edge],
            clusters: vec![],
        };
        let arrow_width = |zoom: f32| {
            let mut s = scene.clone();
            // A single node marks the zoom (`h = NODE_WORLD_H × zoom`); it is
            // off-canvas so it does not disturb the arrow measurement.
            s.nodes.push(NodeSpec {
                x: -1000.0,
                y: -1000.0,
                w: 10.0,
                h: crate::layout::NODE_WORLD_H * zoom,
                radius: 0.0,
                fill: (20, 20, 20),
                border: (200, 200, 200),
                border_width: 1.0,
                label: String::new(),
                label_color: (255, 255, 255),
                circuit: String::new(),
                instance_index: 0,
                input_port: false,
                output_port: false,
            });
            let out = paint(Some(&s), egui::vec2(400.0, 200.0));
            assert_eq!(out.polygons, 1, "one arrow polygon");
            // Derive the triangle width from the emitted path bounds.
            let mut minx = f32::INFINITY;
            let mut maxx = f32::NEG_INFINITY;
            let ctx = egui::Context::default();
            let mut full = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 200.0),
                    )),
                    ..Default::default()
                },
                |ui| paint_scene(ui, egui::vec2(400.0, 200.0), Some(&s), &[]),
            );
            for cs in &full.shapes {
                if let egui::epaint::Shape::Path(p) = &cs.shape {
                    for pt in &p.points {
                        minx = minx.min(pt.x);
                        maxx = maxx.max(pt.x);
                    }
                }
            }
            full.textures_delta.clear();
            assert!(minx.is_finite() && maxx.is_finite(), "arrow path emitted");
            maxx - minx
        };

        let wide = arrow_width(1.0);
        let narrow = arrow_width(0.2);
        assert!(
            wide > narrow + 1.0,
            "arrow shrinks with zoom: {wide} -> {narrow}"
        );
        // Never below the visibility floor (2 px length along the tangent).
        assert!(arrow_width(0.001) >= 2.0 - 1e-3);
    }

    // ── sections-black-hole-solver: no cluster boxes in sections mode ───────

    /// Four nodes in two banner groups (nodes 0..2 and 2..4) at world positions
    /// a fitted camera frames.
    /// The shared four-node, two-cluster graph the scene and window tests both
    /// frame: one definition in `test_support`, so the two suites cannot drift
    /// apart (they had already diverged in how they seeded the app).
    fn two_group_app() -> App {
        crate::test_support::two_group_app()
    }

    fn scene_of(app: &App) -> SceneSpec {
        build_scene_spec(
            app,
            theme(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
        )
        .expect("scene present")
    }

    /// Painted rect geometry (frame, fill, stroke color) so two frames can be
    /// compared shape-for-shape without touching egui mesh internals.
    fn rect_geometry(
        rects: &[egui::epaint::RectShape],
    ) -> Vec<(egui::Rect, egui::Color32, egui::Color32)> {
        rects
            .iter()
            .map(|r| (r.rect, r.fill, r.stroke.color))
            .collect()
    }

    /// The minimap panel's rect for a canvas, when the scene lays one out.
    fn map_panel_rect(scene: &SceneSpec, canvas: egui::Vec2) -> Option<egui::Rect> {
        minimap_layout(scene, egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)).map(|m| {
            egui::Rect::from_min_size(
                egui::pos2(m.panel.0, m.panel.1),
                egui::vec2(m.panel.2, m.panel.3),
            )
        })
    }

    /// Painted rects outside the minimap panel, as `(rect, fill, stroke)`: the
    /// map is chrome that now draws on every scene (a fitting layout included),
    /// so scene-shape comparisons measure what the scene itself paints.
    fn scene_rect_geometry(
        scene: &SceneSpec,
        canvas: egui::Vec2,
        rects: &[egui::epaint::RectShape],
    ) -> Vec<(egui::Rect, egui::Color32, egui::Color32)> {
        let panel = map_panel_rect(scene, canvas);
        rect_geometry(rects)
            .into_iter()
            .filter(|(rect, _, _)| panel.is_none_or(|p| !p.contains_rect(*rect)))
            .collect()
    }

    #[test]
    fn paint_omits_cluster_boxes_in_sections_mode() {
        let canvas = egui::vec2(1280.0, 800.0);

        // Positive control: with the clusters in the spec, column paints extra
        // shapes — the two boxes and their titles.
        let mut column = two_group_app();
        column.layout_mode = LayoutMode::Column;
        let column_scene = scene_of(&column);
        let with_boxes = paint(Some(&column_scene), canvas);
        let mut stripped = scene_of(&column);
        stripped.clusters.clear();
        let without_boxes = paint(Some(&stripped), canvas);
        assert!(
            with_boxes.rects.len() > without_boxes.rects.len(),
            "column paints its cluster boxes: {} vs {} rects",
            with_boxes.rects.len(),
            without_boxes.rects.len()
        );
        assert!(
            with_boxes.labels.len() > without_boxes.labels.len(),
            "column paints its cluster titles: {:?}",
            with_boxes.labels
        );

        // Sections mode paints exactly the stripped column frame: same nodes,
        // same cables, and neither a banner-group rectangle nor a cluster
        // title (spec "No cluster boxes drawn in sections mode"). The minimap
        // is compared apart: it draws on every scene, and its dots follow the
        // two arrangements' own positions.
        let mut sections = two_group_app();
        sections.layout_mode = LayoutMode::Sections;
        let sections_scene = scene_of(&sections);
        assert!(sections_scene.clusters.is_empty());
        let sections_out = paint(Some(&sections_scene), canvas);
        assert_eq!(
            sections_out.labels, without_boxes.labels,
            "no cluster title is painted in sections mode"
        );
        assert_eq!(
            scene_rect_geometry(&sections_scene, canvas, &sections_out.rects),
            scene_rect_geometry(&stripped, canvas, &without_boxes.rects),
            "no cluster rectangle is painted in sections mode"
        );
        assert_eq!(sections_out.beziers, without_boxes.beziers);
    }

    // ── cluster-title-legibility: fixed screen-space titles (task 2.2) ──────

    /// Fonts of the painted texts whose content is one of `titles` — the
    /// cluster titles, told apart from node labels by their own strings.
    fn title_fonts(out: &PaintOutput, titles: &[&str]) -> Vec<egui::FontId> {
        out.labels
            .iter()
            .zip(&out.label_fonts)
            .filter(|(label, _)| titles.contains(&label.as_str()))
            .map(|(_, font)| font.clone())
            .collect()
    }

    /// Every cluster box in `scene` has room for a title, so a title missing
    /// from a paint run was omitted by the painter, not by the gate.
    fn assert_boxes_have_title_room(scene: &SceneSpec, zoom: f32) {
        assert!(!scene.clusters.is_empty(), "scene has clusters to gate");
        for cluster in &scene.clusters {
            assert!(
                cluster.h >= CLUSTER_TITLE_MIN_H && cluster.w >= CLUSTER_TITLE_MIN_W,
                "zoom {zoom}: box {:?} must have title room (h {}, w {})",
                cluster.title,
                cluster.h,
                cluster.w
            );
        }
    }

    /// Spec "Cluster title size independent of zoom" (design D4): the painted
    /// cluster-title font is `CLUSTER_TITLE_PX` at camera zoom 0.5 and 1.5, and
    /// identical between the two runs — the boxes scale with the camera, the
    /// title font does not.
    #[test]
    fn cluster_title_font_size_is_identical_across_zoom() {
        let canvas = egui::vec2(1280.0, 800.0);
        let titles = ["Controls", "Audio"];
        let mut runs = Vec::new();
        for &zoom in &[0.5_f32, 1.5] {
            let mut app = two_group_app();
            app.graph_camera = Some(GraphCamera {
                zoom,
                pan: (0.0, 0.0),
            });
            let scene = scene_of(&app);
            assert_boxes_have_title_room(&scene, zoom);
            let out = paint(Some(&scene), canvas);
            let fonts = title_fonts(&out, &titles);
            assert_eq!(
                fonts.len(),
                titles.len(),
                "zoom {zoom}: every box with room paints its title: {:?}",
                out.labels
            );
            for font in &fonts {
                assert_eq!(
                    *font,
                    egui::FontId::proportional(CLUSTER_TITLE_PX),
                    "zoom {zoom}: a cluster title paints at the fixed screen-space font"
                );
            }
            runs.push(fonts);
        }
        assert_eq!(runs[0], runs[1], "the title font is identical across zoom");
    }

    /// Spec "Cluster titles stay readable at fit zoom" (design D1/D4). At zoom
    /// 0.2 the zoom-derived size is `CLUSTER_TITLE_PX × zoom = 2.4 px`, below
    /// the pre-change painter's `raw >= 7 px` floor, while the projected box
    /// still clears the box-room gate. That painter therefore omitted these
    /// titles outright, so the assertion below is precisely the behaviour
    /// change this change exists for, not a tautology.
    #[test]
    fn cluster_title_paints_below_the_old_zoom_floor() {
        /// The deleted `MIN_LABEL_PX` cluster-title floor the old gate compared
        /// the zoom-scaled size against.
        const OLD_MIN_LABEL_PX: f32 = 7.0;
        const ZOOM: f32 = 0.2;

        // The pre-change rule hid every title at this zoom.
        const { assert!(CLUSTER_TITLE_PX * ZOOM < OLD_MIN_LABEL_PX) };

        let canvas = egui::vec2(1280.0, 800.0);
        let titles = ["Controls", "Audio"];
        let mut app = two_group_app();
        // A negative pan keeps the zoomed-down scene inside the canvas.
        app.graph_camera = Some(GraphCamera {
            zoom: ZOOM,
            pan: (-20.0, -20.0),
        });
        let scene = scene_of(&app);
        assert_boxes_have_title_room(&scene, ZOOM);
        let out = paint(Some(&scene), canvas);
        let fonts = title_fonts(&out, &titles);
        assert_eq!(
            fonts.len(),
            titles.len(),
            "a box with room paints its title at zoom {ZOOM}: {:?}",
            out.labels
        );
        for font in &fonts {
            assert_eq!(
                *font,
                egui::FontId::proportional(CLUSTER_TITLE_PX),
                "below the old floor the title still paints at the fixed font"
            );
        }
    }

    /// Spec "Cluster titles yield to boxes with no room" (design D1/D4): a box
    /// shorter than one title line plus the top inset, and a box narrower than a
    /// readable title fragment, both paint their rectangle but no title text.
    #[test]
    fn cluster_title_yields_to_a_box_without_room() {
        let short = ClusterSpec {
            x: 20.0,
            y: 20.0,
            w: 200.0,
            h: CLUSTER_TITLE_MIN_H - 1.0,
            title: "Squashed".into(),
            border: (200, 200, 255),
            title_color: (255, 255, 255),
            member_indices: vec![],
        };
        let narrow = ClusterSpec {
            x: 20.0,
            y: 80.0,
            w: CLUSTER_TITLE_MIN_W - 1.0,
            h: 60.0,
            title: "Clipped".into(),
            border: (200, 200, 255),
            title_color: (255, 255, 255),
            member_indices: vec![],
        };
        let scene = SceneSpec {
            background: (10, 10, 10),
            nodes: vec![],
            edges: vec![],
            clusters: vec![short.clone(), narrow.clone()],
        };
        let out = paint(Some(&scene), egui::vec2(400.0, 200.0));
        assert!(
            !out.labels.iter().any(|l| l == "Squashed" || l == "Clipped"),
            "no title paints without room: {:?}",
            out.labels
        );
        for cluster in [&short, &narrow] {
            assert!(
                out.rects.iter().any(|r| {
                    (r.rect.left() - cluster.x).abs() < 0.5
                        && (r.rect.top() - cluster.y).abs() < 0.5
                        && (r.rect.width() - cluster.w).abs() < 0.5
                        && (r.rect.height() - cluster.h).abs() < 0.5
                        && r.fill == egui::Color32::TRANSPARENT
                }),
                "the box for {:?} still paints",
                cluster.title
            );
        }
    }

    // ── graph-same-type-grouping: grouped paint shapes (task 4.2) ───────────

    /// The smallest real arrangement that groups under the default settings: an
    /// `App` over five `copy` nodes stacked one body apart in a column, each
    /// feeding the next, with two of them also feeding the same `mixer`.
    ///
    /// `App::new` seeds `graph_grouping` from `crate::config::Graph::default()`
    /// (`group_enabled`, `min_group_size = 5`), and the copies sit inside one
    /// body height of each other, so drawn proximity alone merges them into
    /// group 0 while the lone mixer stays an individual — no extra setup needed
    /// to reach the collapsed path. The cable set doubles as the aggregation
    /// case: four cables wholly inside the group, plus two members fanning out
    /// to one sink.
    fn stacked_copies_app() -> App {
        let mut nodes: Vec<GraphNode> = (0..5).map(|i| circuit_node("copy", i, 0)).collect();
        nodes.push(circuit_node("mixer", 0, 1));
        let copy_cable = |name: &str, from: usize, to: usize| GraphEdge {
            cable: name.to_string(),
            source: NodeId::circuit("copy", from),
            sink: NodeId::circuit("copy", to),
        };
        let fan_cable = |name: &str, from: usize| GraphEdge {
            cable: name.to_string(),
            source: NodeId::circuit("copy", from),
            sink: NodeId::circuit("mixer", 0),
        };
        let graph = Graph {
            nodes,
            edges: vec![
                copy_cable("_CHAIN0", 0, 1),
                copy_cable("_CHAIN1", 1, 2),
                copy_cable("_CHAIN2", 2, 3),
                copy_cable("_CHAIN3", 3, 4),
                fan_cable("_FAN_A", 0),
                fan_cable("_FAN_B", 1),
            ],
            ..Graph::default()
        };
        let mut positions: Vec<(f32, f32)> = (0..5)
            .map(|i| (120.0, 80.0 + i as f32 * crate::layout::NODE_WORLD_H))
            .collect();
        positions.push((460.0, 80.0));

        let mut app = App::new();
        app.graph = Some(graph);
        app.graph_positions = positions;
        app
    }

    /// The scene's one group stand-in, told apart by its `<circuit> ×<count>`
    /// label.
    fn group_body(scene: &SceneSpec) -> &NodeSpec {
        let mut found = scene.nodes.iter().filter(|n| n.label.contains('\u{d7}'));
        let body = found.next().expect("the scene draws a group stand-in");
        assert!(found.next().is_none(), "exactly one group body is drawn");
        body
    }

    /// Spec "A collapsed group draws one box" (design D1/D5): the five members
    /// paint as a single `<circuit> ×<count>` body, and no member body appears
    /// beside it.
    #[test]
    fn collapsed_group_paints_one_box_and_no_member_nodes() {
        let app = stacked_copies_app();
        let scene = scene_of(&app);

        assert_eq!(
            scene.nodes.len(),
            2,
            "the five copies collapse to one box beside the lone mixer: {:?}",
            scene
                .nodes
                .iter()
                .map(|n| n.label.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(group_body(&scene).label, "copy \u{d7}5");
        assert!(
            !scene
                .nodes
                .iter()
                .any(|n| n.label == "copy" || n.label.starts_with("copy (")),
            "no member body is drawn beside the box"
        );
        assert!(
            scene.clusters.is_empty(),
            "a collapsed group is already one body, so it gets no outline"
        );

        // The painted run is the box and the mixer, nothing per member — and
        // the label is complete at this frame width, not ellipsized.
        let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
        let labels: Vec<&str> = out.labels.iter().map(String::as_str).collect();
        assert_eq!(labels, ["copy \u{d7}5", "mixer"]);
    }

    /// Spec "An expanded group draws its members" (design D5): the members
    /// paint as their own bodies, and the group survives as one outline titled
    /// like the collapsed box.
    #[test]
    fn expanded_group_paints_its_members_and_the_outline() {
        let mut app = stacked_copies_app();
        app.expanded_groups.insert(0);
        let scene = scene_of(&app);

        assert_eq!(scene.nodes.len(), 6, "every member draws on its own");
        let labels: Vec<&str> = scene.nodes.iter().map(|n| n.label.as_str()).collect();
        for member in ["copy", "copy (1)", "copy (2)", "copy (3)", "copy (4)"] {
            assert!(
                labels.contains(&member),
                "an expanded group draws {member:?}: {labels:?}"
            );
        }

        assert_eq!(scene.clusters.len(), 1, "one outline around the members");
        let outline = &scene.clusters[0];
        assert_eq!(outline.title, "copy \u{d7}5");
        assert_eq!(outline.member_indices.len(), 5);
        for &idx in &outline.member_indices {
            let body = &scene.nodes[idx];
            assert!(
                body.x >= outline.x
                    && body.x + body.w <= outline.x + outline.w
                    && body.y >= outline.y
                    && body.y + body.h <= outline.y + outline.h,
                "the outline encloses member body {idx}"
            );
        }

        let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
        for member in ["copy", "copy (1)", "copy (2)", "copy (3)", "copy (4)"] {
            assert!(
                out.labels.iter().any(|l| l == member),
                "the member body {member:?} paints: {:?}",
                out.labels
            );
        }
        assert_eq!(
            title_fonts(&out, &["copy \u{d7}5"]),
            vec![egui::FontId::proportional(CLUSTER_TITLE_PX)],
            "the outline paints its group label: {:?}",
            out.labels
        );
        assert!(
            out.rects.iter().any(|r| {
                (r.rect.left() - outline.x).abs() < 0.5
                    && (r.rect.top() - outline.y).abs() < 0.5
                    && r.fill == egui::Color32::TRANSPARENT
            }),
            "the outline rectangle paints"
        );
    }

    /// Spec "Aggregated edges" (design D3): a cable whose ends land inside one
    /// collapsed group draws nothing, and the two members feeding the same sink
    /// collapse onto one arrow.
    #[test]
    fn aggregated_edges_emit_one_arrow_per_drawn_pair() {
        let app = stacked_copies_app();
        assert_eq!(
            app.graph.as_ref().unwrap().edges.len(),
            6,
            "fixture guard: four intra-group cables and two fan-out cables"
        );
        let scene = scene_of(&app);
        assert_eq!(
            scene.edges.len(),
            1,
            "the four intra-group cables drop and the duplicate fan-out deduplicates"
        );

        let box_body = group_body(&scene);
        let mixer = scene
            .nodes
            .iter()
            .find(|n| n.label == "mixer")
            .expect("the mixer body draws on its own");
        let edge = &scene.edges[0];
        assert!(
            (edge.start.0 - (box_body.x + box_body.w)).abs() < 0.01,
            "the one arrow leaves the group box: {edge:?}"
        );
        assert!((edge.start.1 - (box_body.y + box_body.h / 2.0)).abs() < 0.01);
        assert!(
            (edge.end.0 - mixer.x).abs() < 0.01,
            "and lands on the mixer: {edge:?}"
        );
        assert!((edge.end.1 - (mixer.y + mixer.h / 2.0)).abs() < 0.01);

        let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
        assert_eq!(out.beziers, 1, "one aggregated cable stroke");
        assert_eq!(out.polygons, 1, "one direction arrow");
    }

    /// Task 4.1 keeps the painter's level-of-detail rule for group labels: a box
    /// narrower than its `<circuit> ×<count>` label paints the ellipsized text
    /// instead — asserted on the emitted `Shape::Text`, not on `fit_label`.
    #[test]
    fn overlong_group_label_ellipsizes_to_its_frame() {
        // The box's width at zoom 1, so the zoom below lands it on a chosen
        // pixel width instead of a guessed one.
        let full_w = group_body(&scene_of(&stacked_copies_app())).w;
        let mut app = stacked_copies_app();
        // 40 px: wider than one label character (so the label still paints) and
        // narrower than the seven characters the full label needs at the fixed
        // label font.
        app.graph_camera = Some(GraphCamera {
            zoom: 40.0 / full_w,
            pan: (0.0, 0.0),
        });
        let scene = scene_of(&app);

        let body = group_body(&scene);
        assert_eq!(body.label, "copy \u{d7}5", "the spec keeps the full label");
        let char_w = MONO_ADVANCE * NODE_LABEL_SIZE;
        assert!(
            body.w >= char_w && body.w < body.label.chars().count() as f32 * char_w,
            "the painted frame must be narrower than the label: w {} for {:?}",
            body.w,
            body.label
        );

        let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
        let painted: Vec<&str> = out
            .labels
            .iter()
            .map(String::as_str)
            .filter(|l| l.starts_with("copy"))
            .collect();
        assert_eq!(painted.len(), 1, "one group label paints: {:?}", out.labels);
        let text = painted[0];
        assert!(
            text.ends_with('\u{2026}'),
            "the over-long label ellipsizes: {text:?}"
        );
        assert_ne!(
            text, "copy \u{d7}5",
            "the full label does not fit the frame"
        );
        let budget = (body.w / char_w).floor() as usize;
        assert_eq!(
            text.chars().count(),
            budget,
            "the painted label spends the whole frame budget: {text:?} in {} px",
            body.w
        );
        assert!(
            text.chars().count() as f32 * char_w <= body.w + 1e-3,
            "the painted label fits its frame: {text:?} in {} px",
            body.w
        );
    }

    // ── graph-same-type-grouping: hover content peek (task 5.2) ──────────────

    /// Task 5.2's line rule: a shared label prefix collapses into the range its
    /// instances span; without one the first eight labels stand, and the rest is
    /// summarized as `…+N`.
    #[test]
    fn group_peek_line_ranges_a_shared_prefix_and_lists_the_rest() {
        assert_eq!(
            group_peek_line(&["encoder E1.1".to_string(), "encoder E1.24".to_string()]),
            "encoder \u{b7} E1.1\u{2013}E1.24",
            "the prefix names the run, the range carries its extent"
        );

        let numbered: Vec<String> = (1..=10).map(|i| format!("copy ({i})")).collect();
        assert_eq!(
            group_peek_line(&numbered),
            "copy (1) \u{b7} copy (2) \u{b7} copy (3) \u{b7} copy (4) \u{b7} copy (5) \u{b7} \
             copy (6) \u{b7} copy (7) \u{b7} copy (8) \u{b7} \u{2026}+2",
            "eight labels and a remainder count"
        );

        // One member's label *is* the shared prefix (the numbered-instance case),
        // so there is nothing to range and the list form reads it.
        assert_eq!(
            group_peek_line(&["copy".to_string(), "copy (1)".to_string()]),
            "copy \u{b7} copy (1)"
        );
    }

    /// Spec "Hover peeks at contents" (task 5.2): the pointer resting on a
    /// collapsed group's box paints the members' peek in the tooltip card, and
    /// paints no such card when no group is hovered.
    #[test]
    fn hovered_group_peeks_at_its_member_labels() {
        let mut app = stacked_copies_app();
        let scene = scene_of(&app);
        let canvas = egui::vec2(1280.0, 800.0);
        let body = group_body(&scene);
        let centre = egui::pos2(body.x + body.w / 2.0, body.y + body.h / 2.0);
        let peek = "copy \u{b7} copy (1) \u{b7} copy (2) \u{b7} copy (3) \u{b7} copy (4)";
        let pointer = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
            events: vec![egui::Event::PointerMoved(centre)],
            ..Default::default()
        };
        let frame = |app: Option<&App>| {
            paint_ui(pointer(), |ui| {
                paint_scene_in(
                    ui,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
                    Some(&scene),
                    &[],
                    app,
                );
            })
        };

        let quiet = frame(None);
        assert!(
            !quiet.labels.iter().any(|label| label == peek),
            "no hovered group, no peek: {:?}",
            quiet.labels
        );

        app.hovered_group = Some(0);
        let peeked = frame(Some(&app));
        assert!(
            peeked.labels.iter().any(|label| label == peek),
            "the hovered group names its members: {:?}",
            peeked.labels
        );
        assert_eq!(
            peeked.labels.iter().filter(|label| *label == peek).count(),
            1,
            "one card, not one per painted member"
        );
        // The peek takes the card's slot rather than stacking on it, so the
        // painted rect count is unchanged (the layout is untouched) and the last
        // rect — the card, painted above everything — got wider.
        assert_eq!(
            peeked.rects.len(),
            quiet.rects.len(),
            "the peek replaces the node readout instead of drawing beside it"
        );
        let card_width = |out: &PaintOutput| out.rects.last().expect("a card rect").rect.width();
        assert!(
            card_width(&peeked) > card_width(&quiet),
            "the peek card is wider than the readout it replaced: {} vs {}",
            card_width(&peeked),
            card_width(&quiet)
        );
    }

    // ── graph-same-type-grouping: group-level highlights (task 6.1) ─────────

    /// A collapsed group wears the strongest class any member would wear:
    /// selection and the diff classification light it, and a topology error on a
    /// member's cable outranks them both. Insertion order is checked too — the
    /// class does not depend on which member carries it.
    #[test]
    fn collapsed_group_wears_the_strongest_member_class() {
        let mut app = stacked_copies_app();
        assert_eq!(
            group_body(&scene_of(&app)).border,
            theme().rgb(theme().graph_node_border),
            "no member class: the circuit-kind frame"
        );

        app.selected_circuit = Some(NodeId::circuit("copy", 2));
        let selected_scene = scene_of(&app);
        let selected = group_body(&selected_scene);
        assert_eq!(
            selected.border,
            theme().rgb(theme().graph_node_highlight),
            "a selected member lights the whole box"
        );
        assert_eq!(
            selected.label_color,
            theme().rgb(theme().graph_node_highlight),
            "the label travels with the selection class"
        );

        app.selected_circuit = None;
        app.diff_showing = true;
        app.diff_report = Some(DiffReport {
            removed_nodes: vec![NodeId::circuit("copy", 4)],
            ..Default::default()
        });
        let diffed_scene = scene_of(&app);
        let diffed = group_body(&diffed_scene);
        assert_eq!(
            diffed.border,
            theme().rgb(theme().graph_edge_diff_removed),
            "a removed member puts the group in the removed class"
        );
        assert_eq!(
            diffed.label_color,
            theme().rgb(theme().graph_edge_diff_removed),
            "the label travels with the class and stays legible"
        );

        app.graph
            .as_mut()
            .expect("graph")
            .validation
            .push(TopologyIssue {
                cable: "_FAN_B".into(),
                severity: TopologySeverity::Error,
                message: "two producers".into(),
            });
        let errored_scene = scene_of(&app);
        let errored = group_body(&errored_scene);
        assert_eq!(
            errored.border,
            theme().rgb(theme().graph_edge_error),
            "a topology error outranks the diff classification"
        );
        assert_eq!(errored.label_color, theme().rgb(theme().graph_edge_error));
    }

    /// Every diff class the report can carry, and their ranking (task 6.2): a
    /// changed member wears the added token exactly as a changed cable does, and
    /// added outranks removed outranks changed, so a group holding several
    /// classes answers with the strongest one. The removed-only case is pinned
    /// by `collapsed_group_wears_the_strongest_member_class` above.
    #[test]
    fn collapsed_group_diff_classes_rank_added_over_removed_over_changed() {
        let mut app = stacked_copies_app();
        app.diff_showing = true;

        app.diff_report = Some(DiffReport {
            added_nodes: vec![NodeId::circuit("copy", 3)],
            ..Default::default()
        });
        assert_eq!(
            group_body(&scene_of(&app)).border,
            theme().rgb(theme().graph_edge_diff_added),
            "an added member puts the group in the added class"
        );

        app.diff_report = Some(DiffReport {
            changed_nodes: vec![ChangedNode {
                id: NodeId::circuit("copy", 1),
                changed_params: vec!["input".into()],
            }],
            ..Default::default()
        });
        assert_eq!(
            group_body(&scene_of(&app)).border,
            theme().rgb(theme().graph_edge_diff_added),
            "a changed member wears the added token, like a changed cable"
        );

        app.diff_report = Some(DiffReport {
            added_nodes: vec![NodeId::circuit("copy", 4)],
            removed_nodes: vec![NodeId::circuit("copy", 0)],
            changed_nodes: vec![ChangedNode {
                id: NodeId::circuit("copy", 1),
                changed_params: vec!["input".into()],
            }],
            ..Default::default()
        });
        let ranked = scene_of(&app);
        assert_eq!(
            group_body(&ranked).border,
            theme().rgb(theme().graph_edge_diff_added),
            "added outranks removed and changed when the group holds several"
        );
    }

    /// The influence wash lights a collapsed group through any member, and a
    /// topology error on a member's cable outranks every other source at once —
    /// selection, diff and influence (task 6.2).
    #[test]
    fn collapsed_group_topology_error_outranks_selection_diff_and_influence() {
        let mut app = stacked_copies_app();
        app.influence = Some(InfluenceSubtree {
            influenced_nodes: std::iter::once(NodeId::circuit("copy", 1)).collect(),
            ..Default::default()
        });
        assert_eq!(
            group_body(&scene_of(&app)).border,
            theme().rgb(theme().graph_node_highlight),
            "an influenced member lights the group like a selected one"
        );

        app.selected_circuit = Some(NodeId::circuit("copy", 2));
        app.diff_showing = true;
        app.diff_report = Some(DiffReport {
            added_nodes: vec![NodeId::circuit("copy", 3)],
            ..Default::default()
        });
        app.graph
            .as_mut()
            .expect("graph")
            .validation
            .push(TopologyIssue {
                cable: "_FAN_B".into(),
                severity: TopologySeverity::Error,
                message: "two producers".into(),
            });
        let scene = scene_of(&app);
        let body = group_body(&scene);
        assert_eq!(
            body.border,
            theme().rgb(theme().graph_edge_error),
            "a topology error outranks selection, diff and influence together"
        );
        assert_eq!(
            body.label_color,
            theme().rgb(theme().graph_edge_error),
            "the label travels with the error class"
        );
    }

    /// The emitted `Shape::Text` colour, not only the `NodeSpec`: the painter
    /// hands `label_color` straight to the galley, so the class colour a reader
    /// sees is asserted on the painted frame (task 6.2's "label colour travels
    /// with the strongest class").
    #[test]
    fn group_label_paints_in_the_strongest_class_colour() {
        fn painted(app: &App) -> egui::Color32 {
            let scene = scene_of(app);
            let out = paint(Some(&scene), egui::vec2(1280.0, 800.0));
            let mut colours = out
                .labels
                .iter()
                .zip(&out.label_colors)
                .filter(|(label, _)| label.contains('\u{d7}'))
                .map(|(_, colour)| *colour);
            let colour = colours.next().expect("the group label paints");
            assert!(colours.next().is_none(), "one group label, one text shape");
            colour
        }

        let mut app = stacked_copies_app();
        assert_eq!(
            painted(&app),
            rgb(theme().rgb(theme().graph_node_title)),
            "nothing lights the group: the kind title token paints"
        );

        app.selected_circuit = Some(NodeId::circuit("copy", 2));
        assert_eq!(
            painted(&app),
            rgb(theme().rgb(theme().graph_node_highlight)),
            "selection paints the title in the highlight token"
        );

        app.selected_circuit = None;
        app.diff_showing = true;
        app.diff_report = Some(DiffReport {
            added_nodes: vec![NodeId::circuit("copy", 4)],
            ..Default::default()
        });
        assert_eq!(
            painted(&app),
            rgb(theme().rgb(theme().graph_edge_diff_added)),
            "a diff class paints the title in its own token"
        );

        app.graph
            .as_mut()
            .expect("graph")
            .validation
            .push(TopologyIssue {
                cable: "_FAN_B".into(),
                severity: TopologySeverity::Error,
                message: "two producers".into(),
            });
        assert_eq!(
            painted(&app),
            rgb(theme().rgb(theme().graph_edge_error)),
            "the topology error paints the title in the error token"
        );
    }
}
#[cfg(test)]
// Small fit test: the camera frames a spread world and carries a finite
// zoom, so the seeded scene is visible in the window.
mod fit_tests {
    use crate::graph_render::GraphCamera;

    /// `(width, height)` of a representative node body: the widest estimated
    /// node plus the fixed world height, mirroring `App::graph_fit_node_world`.
    const NODE_WORLD: (f32, f32) = (200.0, 80.0);

    #[test]
    fn graph_window_fit_camera_frames_a_spread_world() {
        let positions = vec![(0.0, 0.0), (160.0, 0.0), (320.0, 120.0)];
        let camera = super::graph_window_fit_camera(&positions, NODE_WORLD, (1280.0, 800.0));
        assert!(camera.zoom.is_finite());
        assert!(camera.zoom > 0.0);
        assert!(camera.pan.0.is_finite() && camera.pan.1.is_finite());
    }

    /// The node extent must widen the framed span so the whole box of an edge
    /// node (position + extent) stays inside the viewport, not only its
    /// top-left position.
    #[test]
    fn graph_window_fit_camera_frames_whole_node_bodies() {
        let positions = vec![(0.0, 0.0), (320.0, 120.0)];
        let viewport = (1280.0, 800.0);
        let camera = super::graph_window_fit_camera(&positions, NODE_WORLD, viewport);

        // Each node's box: top-left is its position, extent NODE_WORLD;
        // every corner must project inside the viewport.
        for &(wx, wy) in &positions {
            let corners = [
                (wx, wy),
                (wx + NODE_WORLD.0, wy),
                (wx, wy + NODE_WORLD.1),
                (wx + NODE_WORLD.0, wy + NODE_WORLD.1),
            ];
            for &(cx, cy) in &corners {
                let (px, py) = camera.world_to_pixel(cx, cy);
                assert!(
                    px >= -0.5 && px <= viewport.0 + 0.5,
                    "corner x {px} inside 0..{}",
                    viewport.0
                );
                assert!(
                    py >= -0.5 && py <= viewport.1 + 0.5,
                    "corner y {py} inside 0..{}",
                    viewport.1
                );
            }
        }
    }

    /// A larger node extent yields a zoom no greater than a tiny extent's, so
    /// the framed span strictly accounts for the node body.
    #[test]
    fn graph_window_fit_camera_widens_span_for_larger_extent() {
        let positions = vec![(0.0, 0.0), (320.0, 120.0)];
        let viewport = (1280.0, 800.0);
        let small = super::graph_window_fit_camera(&positions, (10.0, 10.0), viewport);
        let large = super::graph_window_fit_camera(&positions, (200.0, 80.0), viewport);
        assert!(
            large.zoom <= small.zoom,
            "larger extent frames a wider span: {} <= {}",
            large.zoom,
            small.zoom
        );
        let _ = GraphCamera::default();
    }
}

// ── egui_kittest integration tests ──────────────────────────────────────────

#[cfg(test)]
mod kittest_tests {
    use crate::app::{App, ViewType};
    use crate::handler::{handle_event, key_modifiers, KeyCode, KeyEvent};
    use crate::patch::Patch;
    use crate::theme;
    use std::path::Path;

    fn setup(patch_name: &str) -> App {
        theme::init(theme::Theme::classic());
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new(&format!("fixtures/{patch_name}"))).unwrap();
        app.load_patch(patch);
        app
    }

    fn key(app: &mut App, code: KeyCode) {
        handle_event(&KeyEvent::new(code, key_modifiers::NONE), app);
    }

    fn render_graph(app: &App) {
        let scene = super::build_scene_spec(
            app,
            theme::active(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
        );
        let canvas = egui::vec2(800.0, 600.0);
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            super::paint_scene(ui, canvas, scene.as_ref(), &[]);
        });
        harness.run();
    }

    /// Repaint the graph through a harness and hand the harness back so the
    /// caller can query widgets by AccessKit label after the frame settles.
    fn render_graph_queryable(app: &App) -> egui_kittest::Harness<'static> {
        let scene = super::build_scene_spec(
            app,
            theme::active(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
        );
        let canvas = egui::vec2(800.0, 600.0);
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            super::paint_scene(ui, canvas, scene.as_ref(), &[]);
        });
        harness.run();
        harness
    }

    /// Drive one key through the winit seam (the conversion entry the desktop
    /// loop uses) and return the quit signal.
    fn winit_key(logical_key: &winit::keyboard::Key, app: &mut App) -> bool {
        crate::handler::handle_window_key_event(
            logical_key,
            winit::event::ElementState::Pressed,
            winit::keyboard::ModifiersState::empty(),
            app,
        )
    }

    #[test]
    fn window_key_event_drives_quit_and_graph_repaint_exposes_node_labels() {
        use egui_kittest::kittest::Queryable;

        let mut app = setup("own_buttons.ini");

        // `q` pressed through the winit seam returns the quit signal end to end.
        let quit = winit_key(&winit::keyboard::Key::Character("q".into()), &mut app);
        assert!(quit, "q pressed via the winit seam must signal quit");

        // `g g` through the same seam opens the graph view.
        winit_key(&winit::keyboard::Key::Character("g".into()), &mut app);
        winit_key(&winit::keyboard::Key::Character("g".into()), &mut app);
        assert!(
            app.pane_holding(ViewType::Graph).is_some(),
            "g g must open the graph view"
        );

        // Repaint and resolve a graph node by its AccessKit label. `get_by_label`
        // panics when the label is absent, so a clean return proves the repaint
        // exposed the node. The copy circuit carries no stored label, so
        // since label-guess-and-screen-scale its title is the cached
        // tree-derived guess (a preamble port label upstream), not the raw
        // circuit name — derive the expected label from the same chain.
        let copy = app
            .graph
            .as_ref()
            .expect("graph open")
            .nodes
            .iter()
            .find(|n| n.circuit == "copy")
            .expect("copy node")
            .id
            .clone();
        let want = app
            .guessed_labels
            .get(&copy)
            .cloned()
            .unwrap_or_else(|| "copy".to_string());
        let harness = render_graph_queryable(&app);
        harness.get_by_label(&want);
    }

    #[test]
    fn graph_renders_without_panic() {
        let mut app = setup("own_buttons.ini");
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('g'));
        assert!(app.pane_holding(ViewType::Graph).is_some());
        render_graph(&app);
    }

    #[test]
    fn graph_has_nodes_after_open() {
        let mut app = setup("own_buttons.ini");
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('g'));
        let graph = app.graph.as_ref().unwrap();
        assert!(!graph.nodes.is_empty());
        assert!(!graph.edges.is_empty());
    }

    #[test]
    fn graph_close_and_reopen() {
        let mut app = setup("own_buttons.ini");
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('g'));
        assert!(app.pane_holding(ViewType::Graph).is_some());
        render_graph(&app);

        key(&mut app, KeyCode::Esc);
        assert!(app.pane_holding(ViewType::Graph).is_none());

        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('g'));
        assert!(app.pane_holding(ViewType::Graph).is_some());
        render_graph(&app);
    }

    #[test]
    fn prefix_g_v_opens_source_viewer() {
        let mut app = setup("own_buttons.ini");
        // The startup pane config (task 1.2) already opens the source viewer in
        // a small pane, so `g v` routes focus to that pane instead of opening a
        // second copy.
        assert!(
            app.pane_holding(ViewType::SourceViewer).is_some(),
            "the startup pane config opens the source viewer"
        );
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('v'));
        assert!(
            app.pane_holding(ViewType::SourceViewer).is_some(),
            "the viewer stays open (no duplicate)"
        );
        assert_eq!(
            app.layout.pane(app.layout.focus).view,
            Some(crate::app::ViewType::SourceViewer),
            "g v routes focus to the source-viewer pane"
        );
    }

    #[test]
    fn esc_closes_source_viewer() {
        let mut app = setup("own_buttons.ini");
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('v'));
        assert!(app.pane_holding(ViewType::SourceViewer).is_some());
        key(&mut app, KeyCode::Esc);
        assert!(app.pane_holding(ViewType::SourceViewer).is_none());
    }

    #[test]
    fn shift_group_toggle() {
        let mut app = setup("own_buttons.ini");
        assert!(app.active_shift.is_none());
        key(&mut app, KeyCode::Char('1'));
        assert!(app.active_shift.is_some());
        key(&mut app, KeyCode::Esc);
        assert!(app.active_shift.is_none());
    }

    #[test]
    fn reload_patch_resets_graph_state() {
        let mut app = setup("own_buttons.ini");
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('g'));
        assert!(app.pane_holding(ViewType::Graph).is_some());

        let patch = Patch::from_ini_file(Path::new("fixtures/own_buttons.ini")).unwrap();
        app.load_patch(patch);
        assert!(app.pane_holding(ViewType::Graph).is_none());
    }

    // ── Journey tests ────────────────────────────────────────────────────────

    #[test]
    fn journey_graph_open_close_reopen() {
        let mut app = setup("own_buttons.ini");
        // Open graph via keyboard (g g)
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('g'));
        assert!(
            app.pane_holding(ViewType::Graph).is_some(),
            "graph should be present after open"
        );
        assert!(app.graph.is_some(), "graph data should exist after open");
        // Render while open
        render_graph(&app);
        // Close graph — close_graph hides the view but preserves graph data
        app.close_graph();
        assert!(
            app.pane_holding(ViewType::Graph).is_none(),
            "graph should be hidden after close"
        );
        // Reopen — graph rebuilds from the same patch
        app.open_graph();
        assert!(
            app.pane_holding(ViewType::Graph).is_some(),
            "graph should be present after reopen"
        );
        assert!(app.graph.is_some(), "graph data should exist after reopen");
        render_graph(&app);
    }

    #[test]
    fn journey_source_viewer_open_close() {
        let mut app = setup("own_buttons.ini");
        // Open source viewer via g v
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('v'));
        assert!(
            app.pane_holding(ViewType::SourceViewer).is_some(),
            "viewer should be showing"
        );
        // Close viewer via Esc
        key(&mut app, KeyCode::Esc);
        assert!(
            app.pane_holding(ViewType::SourceViewer).is_none(),
            "viewer should be hidden after close"
        );
        // Reopen
        key(&mut app, KeyCode::Char('g'));
        key(&mut app, KeyCode::Char('v'));
        assert!(
            app.pane_holding(ViewType::SourceViewer).is_some(),
            "viewer should be showing after reopen"
        );
    }

    #[test]
    fn journey_physical_skeleton_toggle() {
        let mut app = setup("own_buttons.ini");
        // Skeleton is off by default
        assert!(!app.physical_show_skeleton, "skeleton off by default");
        // Toggle on
        app.physical_show_skeleton = !app.physical_show_skeleton;
        assert!(app.physical_show_skeleton, "skeleton on after toggle");
        // Toggle back off
        app.physical_show_skeleton = !app.physical_show_skeleton;
        assert!(
            !app.physical_show_skeleton,
            "skeleton off after second toggle"
        );
    }

    #[test]
    fn journey_optimizer_basic_flow() {
        let app = setup("own_buttons.ini");
        // The optimizer requires a loaded patch with sections
        assert!(app.patch.is_some(), "patch should be loaded");
        let patch = app.patch.as_ref().unwrap();
        assert!(!patch.sections.is_empty(), "patch should have sections");
    }
}

#[cfg(test)]
mod zz_throwaway_dump {
    use crate::app::App;
    use crate::graph::NodeKind;
    use crate::graph_render::WorldBounds;
    use crate::patch::Patch;
    use crate::theme;
    use std::path::Path;

    #[test]
    fn dump_arpeggio_node_kinds_and_positions() {
        theme::init(theme::Theme::classic());
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/own_buttons.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        let g = app.graph.as_ref().unwrap();
        let pos = &app.graph_positions;
        println!(
            "== total nodes {} == positions len {}",
            g.nodes.len(),
            pos.len()
        );
        for (i, (node, p)) in g.nodes.iter().zip(pos.iter()).enumerate() {
            println!(
                "[{i}] kind={:?} id={:?} circuit={} pos=({:.1},{:.1})",
                node.kind, node.id, node.circuit, p.0, p.1
            );
        }
        let bounds = WorldBounds::from_positions(pos);
        println!("== graph_fit_positions(ALL) bounds = {bounds:?}");
        // Distinguish circuit-only vs controller/jack extents.
        let mut circuit_bounds = (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        );
        for (node, p) in g.nodes.iter().zip(pos.iter()) {
            if node.kind == NodeKind::Circuit {
                circuit_bounds.0 = circuit_bounds.0.min(p.0);
                circuit_bounds.1 = circuit_bounds.1.min(p.1);
                circuit_bounds.2 = circuit_bounds.2.max(p.0);
                circuit_bounds.3 = circuit_bounds.3.max(p.1);
            }
        }
        println!("== circuit-only bounds = {circuit_bounds:?}");
    }
}

#[cfg(test)]
mod zz_throwaway_dump2 {
    use crate::app::App;
    use crate::graph_render::{GraphCamera, WorldBounds};
    use crate::patch::Patch;
    use crate::theme;
    use std::path::Path;

    fn build() -> App {
        theme::init(theme::Theme::classic());
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/own_buttons.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        app
    }

    #[test]
    fn dump_scene_under_c_camera() {
        let mut app = build();
        let pos = &app.graph_positions;
        let bounds = WorldBounds::from_positions(pos);
        println!("== ALL bounds {bounds:?}");
        // The C fit: fit_to_world(all bounds, 512x800, GRAPH_FIT_MIN_NODE_PX=2.2)
        let cam = GraphCamera::fit_to_world(bounds, (512.0, 800.0), 2.2);
        println!("== C camera zoom={} pan={:?}", cam.zoom, cam.pan);
        app.graph_camera = Some(cam);
        let spec = crate::gui::graph::build_scene_spec(
            &app,
            theme::active(),
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512.0, 800.0)),
        )
        .unwrap();
        println!(
            "== build_scene_spec rendered {} nodes (of {} graph.nodes)",
            spec.nodes.len(),
            app.graph.as_ref().unwrap().nodes.len()
        );
        for (i, n) in spec.nodes.iter().enumerate() {
            println!(
                "  spec[{i}] label={} x={:.0} y={:.0} w={} h={}",
                n.label, n.x, n.y, n.w, n.h
            );
        }
        println!("== spec edges = {}", spec.edges.len());
    }
}

/// sections-black-hole-solver 4.1/4.3 (and 6.4) plus
/// force-arrangement-nonoverlap-and-sliders 2.2/2.3 (design D7): the sliders
/// panel paints only in the arrangements that have forces (`Force` and
/// `Sections`) with the panel toggled on, its three rows follow the arrangement
/// and bind to the live `App` forces (each showing its numeric value, clamped to
/// the row's own range), the Arrange button's own click region resolves inside
/// the panel chrome and reports the arrangement it arranged, the Cohesion row's
/// ceiling stays inside the solver's clearable cohesion band, and the panel rect
/// consumes the pointer before the canvas marquee.
#[cfg(test)]
mod sliders_panel_tests {
    use super::*;
    use crate::app::App;
    use crate::config::LayoutMode;
    use crate::graph::{Graph, GraphEdge, GraphNode};
    use crate::patch::{NodeId, Patch};
    use std::path::Path;

    /// The graph pane every panel frame paints into.
    fn pane() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 640.0))
    }

    fn circuit_node(name: &str, idx: usize) -> GraphNode {
        GraphNode {
            id: NodeId::circuit(name, idx),
            kind: NodeKind::Circuit,
            circuit: name.to_string(),
            instance_index: idx,
            section_index: idx,
        }
    }

    /// Two chained circuits in sections mode with the sliders panel toggled on:
    /// a graph the panel's Arrange button can re-solve.
    fn panel_app() -> App {
        let graph = Graph {
            nodes: vec![circuit_node("clocktool", 0), circuit_node("osc", 0)],
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
        app.layout_mode = LayoutMode::Sections;
        app.showing_sliders = true;
        app
    }

    fn raw_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(pane()),
            events,
            ..Default::default()
        }
    }

    /// Text labels and rect shapes one painted frame emitted.
    #[derive(Default)]
    struct PanelOutput {
        labels: Vec<String>,
        rects: Vec<egui::Rect>,
    }

    fn collect(full: &mut egui::FullOutput) -> PanelOutput {
        let mut out = PanelOutput::default();
        for cs in &full.shapes {
            match &cs.shape {
                egui::epaint::Shape::Text(t) => out.labels.push(t.galley.text().to_string()),
                egui::epaint::Shape::Rect(r) => out.rects.push(r.rect),
                _ => {}
            }
        }
        full.textures_delta.clear();
        out
    }

    /// Paint one panel frame into its own context and report what it drew.
    fn panel_frame(app: &mut App, events: Vec<egui::Event>) -> PanelOutput {
        let ctx = egui::Context::default();
        let mut full = ctx.run_ui(raw_input(events), |ui| {
            paint_sliders_panel(ui, app, pane());
        });
        collect(&mut full)
    }

    fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    /// The marquee the frame's pointer state produces, under `consume`.
    fn marquee_in(
        scene: &SceneSpec,
        events: Vec<egui::Event>,
        consume: Option<egui::Rect>,
    ) -> Option<MarqueeSelection> {
        let ctx = egui::Context::default();
        let mut got = None;
        let mut full = ctx.run_ui(raw_input(events), |ui| {
            got = ui.ctx().input(|i| frame_marquee(Some(scene), i, consume));
        });
        full.textures_delta.clear();
        got
    }

    #[test]
    fn panel_paints_three_force_sliders_and_arrange_in_sections_mode() {
        let mut app = panel_app();
        // Distinct values so each row's readout proves its own binding.
        app.layout_slice_force = 0.3;
        app.layout_section_force = 0.05;
        app.tension = 0.5;
        let panel = sliders_panel_layout(&app, pane()).expect("panel shown in sections mode");
        let out = panel_frame(&mut app, Vec::new());
        for label in ["Slice", "Section", "Cable", "Arrange"] {
            assert!(
                out.labels.iter().any(|l| l == label),
                "{label} row missing: {:?}",
                out.labels
            );
        }
        for value in ["0.30", "0.05", "0.50"] {
            assert!(
                out.labels.iter().any(|l| l == value),
                "value {value} missing: {:?}",
                out.labels
            );
        }
        // The chrome (backdrop fill, then the border) covers the panel rect.
        assert!(
            out.rects
                .iter()
                .any(|r| r.min == panel.rect.min && r.max == panel.rect.max),
            "panel chrome missing: {:?}",
            out.rects
        );
        // Every row and the button row sit inside the chrome, and each row
        // splits into a label column plus a slider track.
        for (i, row) in panel.rows.iter().enumerate() {
            assert!(panel.rect.contains_rect(*row), "row {i} inside the panel");
            assert!(row.contains_rect(panel.label_rect(i)), "label column {i}");
            assert!(row.contains_rect(panel.slider_rect(i)), "slider track {i}");
        }
        assert!(panel.rect.contains_rect(panel.arrange));
    }

    #[test]
    fn panel_lays_out_in_the_force_and_sections_arrangements_only() {
        // Design D7: the panel belongs to the arrangements that have forces.
        // `Force` joins `Sections` in laying it out; `Column` has no forces and
        // never does, whatever `showing_sliders` says.
        for (mode, showing, lays_out) in [
            (LayoutMode::Column, true, false),
            (LayoutMode::Column, false, false),
            (LayoutMode::Force, true, true),
            (LayoutMode::Force, false, false),
            (LayoutMode::Sections, true, true),
            (LayoutMode::Sections, false, false),
        ] {
            let mut app = panel_app();
            app.layout_mode = mode;
            app.showing_sliders = showing;
            assert_eq!(
                sliders_panel_layout(&app, pane()).is_some(),
                lays_out,
                "{mode:?} / showing_sliders={showing} laid out (or not) wrongly"
            );
            let out = panel_frame(&mut app, Vec::new());
            assert_eq!(
                !out.labels.is_empty(),
                lays_out,
                "{mode:?} / showing_sliders={showing} painted {:?}",
                out.labels
            );
            if !lays_out {
                assert!(
                    out.rects.is_empty(),
                    "{mode:?} / showing_sliders={showing} painted no chrome or rows"
                );
            }
        }
        // A pane too small to hold the panel paints nothing either: the panel
        // would spill over its neighbours.
        let app = panel_app();
        let tiny = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 90.0));
        assert!(sliders_panel_layout(&app, tiny).is_none());
    }

    #[test]
    fn panel_rows_follow_the_arrangement() {
        // Design D7: same geometry, mode-dependent labels and bindings.
        let mut force = panel_app();
        force.layout_mode = LayoutMode::Force;
        force.layout_repulsion_force = 2.0;
        // Inside the row's range: the ceiling is the solver's own measured default
        // (see COHESION_SLIDER_SPAN), so an out-of-band value could not be shown.
        let cohesion = crate::layout::SolveKnobs::default().cohesion;
        force.layout_cohesion_force = cohesion;
        force.tension = 0.5;
        let out = panel_frame(&mut force, Vec::new());
        for label in ["Repulsion", "Cohesion", "Cable", "Arrange"] {
            assert!(
                out.labels.iter().any(|l| l == label),
                "force row {label} missing: {:?}",
                out.labels
            );
        }
        for stale in ["Slice", "Section"] {
            assert!(
                !out.labels.iter().any(|l| l == stale),
                "force panel must not label a sections row {stale}"
            );
        }
        for value in [
            "2.00".to_string(),
            format!("{cohesion:.4}"),
            "0.50".to_string(),
        ] {
            assert!(
                out.labels.contains(&value),
                "force value {value} missing: {:?}",
                out.labels
            );
        }
    }

    #[test]
    fn cohesion_row_ceiling_stays_inside_the_solvers_clearable_band() {
        // Above the solver's own default cohesion the separation pass stops
        // clearing its budget — measured, `0.0069` needs ~380 sweeps and `0.008`
        // never converges at the 128-sweep cap — so the row must never offer that
        // regime, and its strongest setting must still be the measured default
        // (design D3/D7).
        let default = crate::layout::SolveKnobs::default().cohesion;
        let ceiling = cohesion_slider_max();
        assert!(
            ceiling <= default + 1e-9,
            "the Cohesion ceiling {ceiling} exceeds the solver's default {default}"
        );
        assert!(
            ceiling >= default - 1e-9,
            "the solver's default {default} is unreachable inside 0.0..={ceiling}"
        );
        assert!(
            ceiling > 0.0,
            "the Cohesion row must stay draggable, got 0.0..={ceiling}"
        );

        // And the painted row enforces that same ceiling: a knob above it is
        // clamped by the row, so the user cannot select an unclearable value.
        let mut app = panel_app();
        app.layout_mode = LayoutMode::Force;
        let above = default * 100.0;
        app.layout_cohesion_force = above;
        let out = panel_frame(&mut app, Vec::new());
        let shown = format!("{ceiling:.SLIDERS_COHESION_DECIMALS$}");
        assert!(
            out.labels.contains(&shown),
            "the Cohesion row must clamp to its ceiling {shown}: {:?}",
            out.labels
        );
        assert_eq!(
            app.layout_cohesion_force, above,
            "painting the clamped row must not write the clamp back onto the knob"
        );
    }

    #[test]
    fn panel_rows_keep_their_ranges_in_both_force_bearing_arrangements() {
        // Design D7: the geometry is shared, the ranges are per-row, and each row
        // clamps the value it paints. Driving every knob past an endpoint of its
        // own range therefore names that endpoint in the painted output: the
        // sections triple is the regression guard for the shared panel (it must
        // not have been rebound by making the panel arrangement-aware) and the
        // force triple pins the two new rows.
        for mode in [LayoutMode::Sections, LayoutMode::Force] {
            let mut app = panel_app();
            app.layout_mode = mode;
            app.layout_slice_force = 10.0;
            app.layout_section_force = -1.0;
            app.layout_repulsion_force = 10.0;
            app.layout_cohesion_force = -1.0;
            app.tension = -1.0;
            let out = panel_frame(&mut app, Vec::new());
            let shows = |label: &str| out.labels.iter().any(|l| l == label);
            for label in ["Arrange", "Cable"] {
                assert!(
                    shows(label),
                    "{mode:?}: {label} row missing: {:?}",
                    out.labels
                );
            }
            // The shared Cable row keeps the solver's own `[TENSION_MIN,
            // TENSION_MAX]` range, so a below-floor tension paints the floor.
            assert!(
                shows(&format!("{:.2}", crate::layout::TENSION_MIN)),
                "{mode:?}: the Cable row must clamp to TENSION_MIN: {:?}",
                out.labels
            );
            if mode == LayoutMode::Force {
                for label in ["Repulsion", "Cohesion"] {
                    assert!(
                        shows(label),
                        "{mode:?}: {label} row missing: {:?}",
                        out.labels
                    );
                }
                for stale in ["Slice", "Section"] {
                    assert!(
                        !shows(stale),
                        "{mode:?}: stale row {stale}: {:?}",
                        out.labels
                    );
                }
                assert!(
                    shows(&format!("{REPULSION_GAIN_MAX:.2}")),
                    "{mode:?}: the Repulsion row must clamp to REPULSION_GAIN_MAX: {:?}",
                    out.labels
                );
                assert!(
                    shows(&format!("{:.*}", SLIDERS_COHESION_DECIMALS, 0.0)),
                    "{mode:?}: the Cohesion row must clamp to its 0.0 floor: {:?}",
                    out.labels
                );
            } else {
                for label in ["Slice", "Section"] {
                    assert!(
                        shows(label),
                        "{mode:?}: {label} row missing: {:?}",
                        out.labels
                    );
                }
                for stale in ["Repulsion", "Cohesion"] {
                    assert!(
                        !shows(stale),
                        "{mode:?}: stale row {stale}: {:?}",
                        out.labels
                    );
                }
                assert!(
                    shows(&format!("{SLIDERS_FORCE_MAX:.2}")),
                    "{mode:?}: the slice/section rows must keep clamping to SLIDERS_FORCE_MAX: {:?}",
                    out.labels
                );
                assert!(
                    shows(&format!("{:.2}", 0.0)),
                    "{mode:?}: the Section row must clamp to its 0.0 floor: {:?}",
                    out.labels
                );
            }
            // A repaint never writes a row's clamp back onto the knob.
            assert_eq!(
                app.tension, -1.0,
                "{mode:?}: painting must not retune the Cable knob"
            );
        }
    }

    #[test]
    fn dragging_a_sections_row_commits_the_sections_knob_and_re_solves_live() {
        // Design D7's other half: row 0 stays the sections Slice binding when the
        // arrangement is `Sections` — making the panel arrangement-aware must not
        // have rebound it to the force-only Repulsion knob, and the sections drag
        // must still re-solve live.
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/own_buttons.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        app.layout_mode = LayoutMode::Sections;
        app.showing_sliders = true;
        let before = app.graph_positions.clone();
        let repulsion_before = app.layout_repulsion_force;
        assert_eq!(
            app.layout_slice_force,
            crate::config::DEFAULT_SLICE_FORCE,
            "the sections drag test needs the solver's own slice default to move from"
        );
        let panel = sliders_panel_layout(&app, pane()).expect("panel shown in sections mode");
        let track = panel.slider_rect(0);
        let target = egui::pos2(track.min.x + 70.0, track.center().y);
        let ctx = egui::Context::default();
        for events in [
            vec![egui::Event::PointerMoved(target)],
            vec![pointer_button(target, true)],
            vec![pointer_button(target, false)],
        ] {
            let mut full = ctx.run_ui(raw_input(events), |ui| {
                paint_sliders_panel(ui, &mut app, pane());
            });
            full.textures_delta.clear();
        }
        assert!(
            app.layout_slice_force > crate::config::DEFAULT_SLICE_FORCE,
            "the drag must move the Slice knob, got {}",
            app.layout_slice_force
        );
        assert_eq!(
            app.layout_repulsion_force, repulsion_before,
            "a sections drag must not touch the force-only Repulsion knob"
        );
        assert!(
            !app.graph_positions.is_empty(),
            "the sections re-solve must keep a solved layout"
        );
        assert_ne!(
            app.graph_positions, before,
            "a sections re-solve under the new slice force must move the nodes"
        );
    }

    #[test]
    fn painting_the_panel_never_retunes_a_knob_or_re_solves_by_itself() {
        // egui rounds a slider to the row's display precision on every frame and
        // reports that write-back as a change. The measured cohesion default is
        // finer than two decimals, so without the user-edit gate the first
        // painted frame would retune the knob and rebuild the graph.
        let mut app = panel_app();
        app.layout_mode = LayoutMode::Force;
        let cohesion = crate::layout::SolveKnobs::default().cohesion;
        app.layout_cohesion_force = cohesion;
        let positions = app.graph_positions.clone();
        let out = panel_frame(&mut app, Vec::new());
        assert!(
            !out.labels.is_empty(),
            "the force panel painted nothing to test"
        );
        assert_eq!(
            app.layout_cohesion_force, cohesion,
            "a repaint must not retune the cohesion knob"
        );
        assert_eq!(
            app.graph_positions, positions,
            "a repaint must not re-solve the layout"
        );
        // The Cohesion row shows the knob's own value at the row's precision.
        let grid = 0.5 * 10f32.powi(-(SLIDERS_COHESION_DECIMALS as i32));
        assert!(
            out.labels
                .iter()
                .filter_map(|l| l.parse::<f32>().ok())
                .any(|v| (v - cohesion).abs() <= grid + 1e-6),
            "the Cohesion row must show {cohesion} to {SLIDERS_COHESION_DECIMALS} decimals: {:?}",
            out.labels
        );
    }

    #[test]
    fn arrange_button_click_region_is_inside_the_panel_and_anneals() {
        let mut app = panel_app();
        let panel = sliders_panel_layout(&app, pane()).expect("panel shown");
        assert!(
            panel.rect.contains_rect(panel.arrange),
            "the button row is inside the panel chrome"
        );
        let center = panel.arrange.center();
        assert!(panel.rect.contains(center), "and so is its click region");
        // Hover/register, press, release on one context: the egui button click
        // lifecycle.
        let ctx = egui::Context::default();
        for events in [
            vec![egui::Event::PointerMoved(center)],
            vec![pointer_button(center, true)],
            vec![pointer_button(center, false)],
        ] {
            let mut full = ctx.run_ui(raw_input(events), |ui| {
                paint_sliders_panel(ui, &mut app, pane());
            });
            full.textures_delta.clear();
        }
        assert_eq!(
            app.status_message, "Arranged (sections)",
            "the Arrange button click re-solves the sections layout"
        );
    }

    #[test]
    fn arrange_button_re_solves_the_force_layout_from_its_seed() {
        let mut app = panel_app();
        app.layout_mode = LayoutMode::Force;
        app.layout_repulsion_force = 2.0;
        // A perturbed position proves the button re-seeds instead of annealing
        // from what the user currently sees.
        app.graph_positions[0] = (9_000.0, 9_000.0);
        let graph = app.graph.as_ref().unwrap().clone();
        let pins = app.pinned_indices(&graph);
        let expected = crate::layout::solve(&graph, &pins, app.force_solve_knobs());
        let panel = sliders_panel_layout(&app, pane()).expect("panel shown");
        let center = panel.arrange.center();
        let ctx = egui::Context::default();
        for events in [
            vec![egui::Event::PointerMoved(center)],
            vec![pointer_button(center, true)],
            vec![pointer_button(center, false)],
        ] {
            let mut full = ctx.run_ui(raw_input(events), |ui| {
                paint_sliders_panel(ui, &mut app, pane());
            });
            full.textures_delta.clear();
        }
        assert_eq!(
            app.status_message, "Arranged (force)",
            "the Arrange button reports the arrangement it arranged"
        );
        assert_eq!(
            app.graph_positions, expected,
            "force Arrange re-solves from the seed under the live knobs"
        );
    }

    #[test]
    fn dragging_a_force_row_commits_the_knob_and_re_solves_live() {
        // The user-edit gate must not swallow real drags: a press on the
        // Repulsion track commits the value and rebuilds the layout (design D7).
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/own_buttons.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        app.layout_mode = LayoutMode::Force;
        app.showing_sliders = true;
        let before = app.graph_positions.clone();
        let panel = sliders_panel_layout(&app, pane()).expect("panel shown in force mode");
        let track = panel.slider_rect(0);
        // The slider itself is one `slider_width` (100 pt) wide from the track's
        // left edge, so the press goes inside it rather than into the free track.
        let target = egui::pos2(track.min.x + 70.0, track.center().y);
        let ctx = egui::Context::default();
        for events in [
            vec![egui::Event::PointerMoved(target)],
            vec![pointer_button(target, true)],
            vec![pointer_button(target, false)],
        ] {
            let mut full = ctx.run_ui(raw_input(events), |ui| {
                paint_sliders_panel(ui, &mut app, pane());
            });
            full.textures_delta.clear();
        }
        assert!(
            app.layout_repulsion_force > 1.5,
            "the drag must move the Repulsion knob, got {}",
            app.layout_repulsion_force
        );
        assert!(
            !app.graph_positions.is_empty(),
            "the live re-solve must keep a solved layout"
        );
        assert_ne!(
            app.graph_positions, before,
            "a re-solve under the new gain must move the nodes"
        );
    }

    #[test]
    fn panel_rect_consumes_the_pointer_before_the_canvas_marquee() {
        let app = panel_app();
        let panel = sliders_panel_layout(&app, pane()).expect("panel shown");
        let scene = SceneSpec {
            background: (0, 0, 0),
            nodes: vec![],
            edges: vec![],
            clusters: vec![],
        };
        let inside = panel.rect.center();
        let outside = egui::pos2(20.0, 20.0);
        let drag = |from: egui::Pos2| {
            let to = egui::pos2(from.x + 300.0, from.y + 300.0);
            vec![
                egui::Event::PointerMoved(from),
                pointer_button(from, true),
                egui::Event::PointerMoved(to),
            ]
        };
        // Positive control: the drag is a marquee outside the panel…
        assert!(
            marquee_in(&scene, drag(outside), None).is_some(),
            "an empty-canvas drag starts a marquee"
        );
        assert!(
            marquee_in(&scene, drag(outside), Some(panel.rect)).is_some(),
            "the exclusion only covers the panel's own rect"
        );
        // …and never one when the press begins inside the consumed panel rect.
        assert!(
            marquee_in(&scene, drag(inside), None).is_some(),
            "without the exclusion the same press does marquee"
        );
        assert!(
            marquee_in(&scene, drag(inside), Some(panel.rect)).is_none(),
            "a press inside the panel never starts a marquee"
        );
    }

    #[test]
    fn window_rect_resolves_inside_the_open_graph_pane() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/own_buttons.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        app.layout_mode = LayoutMode::Sections;
        app.showing_sliders = true;
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 768.0));
        let pane = graph_pane_rect(&app, window).expect("the graph pane is open");
        let panel = sliders_panel_window_rect(&app, window).expect("the panel resolves in it");
        assert!(
            pane.contains_rect(panel),
            "panel {panel:?} inside its pane {pane:?}"
        );
        assert_eq!(
            panel,
            sliders_panel_layout(&app, pane).unwrap().rect,
            "window resolution and the painted geometry agree"
        );
        // Hidden: nothing to consume, so the guard is free when the panel is off.
        app.showing_sliders = false;
        assert!(sliders_panel_window_rect(&app, window).is_none());
    }
}
