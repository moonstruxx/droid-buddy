//! Pixel-space camera and offscreen rasterizer for the graph surface.
//!
//! This module is intentionally pure: no terminal, no ratatui, no `App` — only
//! `f32` math over the layout solver's world plane, the kitty image's pixel
//! plane, and `tiny-skia`/`fontdue` rasterization. Task 2.1 wires it into
//! `render_graph` by replacing the bounding-box fit with the camera and
//! deriving `graph_node_rects` through the inverse; task 2.2 supplies the
//! `Color → RGB` hop that feeds `render_scene` via [`build_scene`], the
//! theme-aware leaf converting the classified tokens from `ui.rs`'s pipeline —
//! below that hop everything is RGB-pure.

/// Degenerate-world guard: a zero-span axis (single node, coincident nodes)
/// behaves as if it spanned `MIN_SPAN` so the fit zoom stays finite.
const MIN_SPAN: f32 = 1.0;
/// Zoom floor/ceiling for manual zoom (`zoom_by`); the floor keeps
/// `pixel_to_world` well-defined, the ceiling keeps the anchor pan math within
/// `f32` precision. `fit_to_world` never needs the ceiling — it only raises.
const MIN_ZOOM: f32 = 1e-3;
const MAX_ZOOM: f32 = 1e6;

/// Axis-aligned bounding box of the world (solver) coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl WorldBounds {
    /// Bounding box of solver positions. Empty input collapses to the zero box
    /// at the origin; `fit_to_world` guards zero spans, so a degenerate box
    /// never produces NaN.
    pub fn from_positions(positions: &[(f32, f32)]) -> Self {
        if positions.is_empty() {
            return Self {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
            };
        }
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for &(x, y) in positions {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }
}

/// The pure world→pixel camera of the graph surface (design D7), mirroring the
/// physical view's `ScreenMapping` idiom at pixel resolution. World coordinates
/// are the layout solver's unbounded `f32` plane (`graph_positions`); pixels
/// are the kitty image's RGBA space.
///
/// Transform: `pixel = world × zoom − pan` — `zoom` is pixels per world unit
/// and `pan` is the pixel-space offset of the world origin. The inverse
/// (`pixel_to_world`) feeds hit-testing: world → pixel → cell derives
/// `graph_node_rects` so the existing drag/hover apparatus works unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphCamera {
    /// Pixels per world unit (solver coordinate).
    pub zoom: f32,
    /// Pixel offset of the world origin (`pixel = world × zoom − pan`).
    pub pan: (f32, f32),
}

impl Default for GraphCamera {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCamera {
    /// Identity camera: world unit maps 1:1 to a pixel, world origin at the
    /// viewport's top-left.
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan: (0.0, 0.0),
        }
    }

    /// Initial fit (design D5, width-first): zoom/pan so `world_bounds`
    /// preserves aspect ratio while *preferring to fill the canvas width* — the
    /// fit zoom is `min(pw/span_w, ph/span_h)`, so a wide graph (a horizontal
    /// chain) scales to fill the width exactly, and a tall graph scales to fit
    /// the height with the whole graph framed. The zoom is additionally clamped
    /// up to `min_node_px` pixels per world unit so a spread-out world overflows
    /// rather than collapsing the smallest node below the legible minimum;
    /// `min_node_px = 0` degrades to a pure fit. The spec's "Legible initial
    /// fit" scenario requires the camera to frame the graph — the min fit keeps
    /// every node in view, and the clamp is the only source of overflow.
    pub fn fit_to_world(bounds: WorldBounds, pixel_size: (f32, f32), min_node_px: f32) -> Self {
        let pixel_size = (pixel_size.0.max(1.0), pixel_size.1.max(1.0));
        let span_w = (bounds.max_x - bounds.min_x).max(MIN_SPAN);
        let span_h = (bounds.max_y - bounds.min_y).max(MIN_SPAN);
        let fit_zoom = (pixel_size.0 / span_w).min(pixel_size.1 / span_h);
        let zoom = fit_zoom.max(min_node_px).max(MIN_ZOOM);
        let (cx, cy) = (
            (bounds.min_x + bounds.max_x) / 2.0,
            (bounds.min_y + bounds.max_y) / 2.0,
        );
        let pan = (
            cx * zoom - pixel_size.0 / 2.0,
            cy * zoom - pixel_size.1 / 2.0,
        );
        Self { zoom, pan }
    }

    /// World point → pixel point.
    pub fn world_to_pixel(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.zoom - self.pan.0, y * self.zoom - self.pan.1)
    }

    /// Pixel point → world point (inverse of `world_to_pixel`).
    pub fn pixel_to_world(&self, px: f32, py: f32) -> (f32, f32) {
        ((px + self.pan.0) / self.zoom, (py + self.pan.1) / self.zoom)
    }

    /// Move the view by `(dx_px, dy_px)` pixels: the pan grows, so a fixed
    /// world point's pixel shifts by exactly `−(dx_px, dy_px)` (mirrors
    /// `ScreenMapping::pan`, where content moves opposite to the offset).
    pub fn pan_by(&mut self, dx_px: f32, dy_px: f32) {
        self.pan.0 += dx_px;
        self.pan.1 += dy_px;
    }

    /// Zoom by `factor` about a fixed world anchor: the anchor's world point
    /// stays at the same pixel position (mirrors `ScreenMapping::zoom_about`).
    /// Zoom is clamped to `[MIN_ZOOM, MAX_ZOOM]`; the anchor math uses the
    /// clamped zoom so the invariant holds at the extremes too.
    pub fn zoom_by(&mut self, factor: f32, anchor_world: (f32, f32)) {
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (ax, ay) = anchor_world;
        self.pan = (
            ax * (new_zoom - self.zoom) + self.pan.0,
            ay * (new_zoom - self.zoom) + self.pan.1,
        );
        self.zoom = new_zoom;
    }

    /// Pixel point → surface cell `(col, row)` given the pixel size of one
    /// cell. Cells are relative to the viewport's top-left; the graph area's
    /// cell origin is added by the caller when publishing hit-test rects.
    /// Off-viewport points map to negative cells deterministically (the
    /// handler clamps to the area). Cell dimensions must be positive.
    pub fn pixel_to_cell(&self, px: f32, py: f32, cell_w: f32, cell_h: f32) -> (i32, i32) {
        ((px / cell_w).floor() as i32, (py / cell_h).floor() as i32)
    }

    /// World point → surface cell, composing `world_to_pixel` + `pixel_to_cell`.
    pub fn world_to_cell(&self, x: f32, y: f32, cell_w: f32, cell_h: f32) -> (i32, i32) {
        let (px, py) = self.world_to_pixel(x, y);
        self.pixel_to_cell(px, py, cell_w, cell_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f32, b: f32, tol: f32) {
        assert!((a - b).abs() <= tol, "expected {a} within {tol} of {b}");
    }

    #[test]
    fn world_pixel_round_trip_stays_within_tolerance() {
        // Identity camera: exact.
        let cam = GraphCamera::new();
        let (wx, wy) = (123.5, -77.25);
        let (px, py) = cam.world_to_pixel(wx, wy);
        let (rx, ry) = cam.pixel_to_world(px, py);
        assert_close(rx, wx, 1e-3);
        assert_close(ry, wy, 1e-3);

        // Fitted camera with non-trivial zoom + pan: round trip within float
        // tolerance of the transform.
        let bounds = WorldBounds {
            min_x: -10.0,
            min_y: -5.0,
            max_x: 30.0,
            max_y: 15.0,
        };
        let cam = GraphCamera::fit_to_world(bounds, (640.0, 400.0), 4.0);
        let (wx, wy) = (7.0, -2.0);
        let (px, py) = cam.world_to_pixel(wx, wy);
        let (rx, ry) = cam.pixel_to_world(px, py);
        assert_close(rx, wx, 1e-2);
        assert_close(ry, wy, 1e-2);
    }

    #[test]
    fn fit_to_world_keeps_smallest_node_legible_and_centers_content() {
        // Spread-out world: 1000 nodes one world unit apart. The raw fit zoom
        // (span 999 → 640px) would shrink a one-unit node to <1px; the minimum
        // clamp must hold the tightest spacing readable instead.
        let positions: Vec<(f32, f32)> = (0..1000).map(|i| (i as f32, (i % 50) as f32)).collect();
        let bounds = WorldBounds::from_positions(&positions);
        let pixel = (640.0, 400.0);
        let min_node_px = 20.0;
        let cam = GraphCamera::fit_to_world(bounds, pixel, min_node_px);

        // The min clamp kicked in: zoom ≥ min_node_px px per world unit.
        assert!(
            cam.zoom >= min_node_px,
            "fit zoom {} shrank nodes below the {min_node_px}px minimum",
            cam.zoom
        );
        // Smallest node (the tightest 1.0-unit gap) renders ≥ min_node_px px.
        assert!(1.0 * cam.zoom >= min_node_px, "tightest gap sub-minimum");
        // Content is centered: the world-bounds center lands on the viewport center.
        let (cx, cy) = (
            (bounds.min_x + bounds.max_x) / 2.0,
            (bounds.min_y + bounds.max_y) / 2.0,
        );
        let (px, py) = cam.world_to_pixel(cx, cy);
        assert_close(px, pixel.0 / 2.0, 1e-2);
        assert_close(py, pixel.1 / 2.0, 1e-2);
    }

    #[test]
    fn zoom_by_keeps_anchor_world_point_fixed_in_pixel_space() {
        let mut cam = GraphCamera::new();
        let anchor = (35.0, -12.0);
        let (ax, ay) = cam.world_to_pixel(anchor.0, anchor.1);
        cam.zoom_by(2.5, anchor);
        let (nx, ny) = cam.world_to_pixel(anchor.0, anchor.1);
        assert_close(nx, ax, 1e-2);
        assert_close(ny, ay, 1e-2);
        assert_close(cam.zoom, 2.5, 1e-3);
    }

    #[test]
    fn pan_by_shifts_pixels_by_expected_delta() {
        let mut cam = GraphCamera::new();
        let (wx, wy) = (10.0, 5.0);
        let (bx, by) = cam.world_to_pixel(wx, wy);
        cam.pan_by(7.0, -3.0);
        let (ax, ay) = cam.world_to_pixel(wx, wy);
        // Pan grows by Δ → a fixed world point's pixel shifts by −Δ.
        assert_close(ax, bx - 7.0, 1e-3);
        assert_close(ay, by + 3.0, 1e-3);
        assert_close(cam.pan.0, 7.0, 1e-3);
        assert_close(cam.pan.1, -3.0, 1e-3);
    }

    #[test]
    fn world_to_pixel_to_cell_round_trip() {
        let cam = GraphCamera::fit_to_world(
            WorldBounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 200.0,
                max_y: 100.0,
            },
            (800.0, 400.0),
            1.0,
        );
        let (wx, wy) = (50.0, 25.0);
        let cell = (12.0, 18.0);
        let (px, py) = cam.world_to_pixel(wx, wy);
        let (col, row) = cam.pixel_to_cell(px, py, cell.0, cell.1);
        // The composed path agrees with the direct pixel→cell path.
        let (col2, row2) = cam.world_to_cell(wx, wy, cell.0, cell.1);
        assert_eq!((col, row), (col2, row2));
        assert_eq!(col, (px / cell.0).floor() as i32);
        assert_eq!(row, (py / cell.1).floor() as i32);
        // Off-viewport points map deterministically to negative cells.
        let (col3, row3) = cam.pixel_to_cell(-5.0, -5.0, cell.0, cell.1);
        assert_eq!((col3, row3), (-1, -1));
    }

    #[test]
    fn cell_center_round_trips_back_to_the_same_cell() {
        // Task 3.2: a world→cell round trip must be stable — the center of a
        // cell, converted back to world and re-projected, lands in the same
        // cell. This is what keeps drag hit-testing on `graph_node_rects`
        // aligned with the pixels `build_scene` rasterizes.
        let cam = GraphCamera::fit_to_world(
            WorldBounds {
                min_x: -50.0,
                min_y: -25.0,
                max_x: 150.0,
                max_y: 75.0,
            },
            (800.0, 400.0),
            2.0,
        );
        let cell = (22.0, 18.0);
        for (wx, wy) in [(37.5, -3.25), (-12.0, 60.0), (0.0, 0.0)] {
            let (col, row) = cam.world_to_cell(wx, wy, cell.0, cell.1);
            // Cell center in pixel space → world → back to a cell.
            let (cx_px, cy_px) = ((col as f32 + 0.5) * cell.0, (row as f32 + 0.5) * cell.1);
            let (wx2, wy2) = cam.pixel_to_world(cx_px, cy_px);
            assert_eq!(
                cam.world_to_cell(wx2, wy2, cell.0, cell.1),
                (col, row),
                "cell center must round-trip to its own cell"
            );
        }
    }

    #[test]
    fn zoom_by_clamps_at_extremes_and_keeps_anchor_fixed() {
        // Task 3.2: the zoom floor/ceiling keep the transform well-defined, and
        // the anchor math uses the clamped zoom, so the anchor invariant holds
        // even when a huge factor pins the camera at an extreme.
        let mut cam = GraphCamera::new();
        let anchor = (12.0, -8.0);
        let (bx, by) = cam.world_to_pixel(anchor.0, anchor.1);
        cam.zoom_by(1e9, anchor); // pins at MAX_ZOOM
        assert_close(cam.zoom, MAX_ZOOM, 1e-3);
        let (ax, ay) = cam.world_to_pixel(anchor.0, anchor.1);
        assert_close(ax, bx, 0.5);
        assert_close(ay, by, 0.5);
        cam.zoom_by(1e-9, anchor); // pins at MIN_ZOOM
        assert_close(cam.zoom, MIN_ZOOM, 1e-3);
        let (ax, ay) = cam.world_to_pixel(anchor.0, anchor.1);
        assert_close(ax, bx, 0.5);
        assert_close(ay, by, 0.5);
    }

    #[test]
    fn fit_prefers_filling_canvas_width() {
        // A wide world — the horizontal-chain case (design D5) — fills the
        // canvas width exactly instead of being centered small: the fit zoom is
        // `pw/span_w` when the graph is proportionally wider than the canvas.
        let bounds = WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 400.0,
            max_y: 100.0,
        };
        let pixel = (1600.0, 640.0); // canvas aspect 2.5 < world aspect 4.0
        let cam = GraphCamera::fit_to_world(bounds, pixel, 0.0);

        assert_close(cam.zoom, 4.0, 1e-3); // pw/span_w fills the width
        let (left, _) = cam.world_to_pixel(0.0, 0.0);
        let (right, _) = cam.world_to_pixel(400.0, 0.0);
        assert_close(left, 0.0, 1e-2);
        assert_close(right, pixel.0, 1e-2);
        // The height fits: the whole chain is framed.
        let (_, bottom) = cam.world_to_pixel(0.0, 100.0);
        assert!(bottom <= pixel.1, "width-first fit keeps the chain framed");
    }

    #[test]
    fn fit_frames_a_taller_than_canvas_world() {
        // A world taller (per width) than the canvas scales to fit the height
        // (the smaller of the two fills), so the whole graph stays in view —
        // the spec's "Legible initial fit" frames the graph.
        let bounds = WorldBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 100.0,
            max_y: 400.0,
        };
        let pixel = (1600.0, 640.0); // canvas aspect 2.5 > world aspect 0.25
        let cam = GraphCamera::fit_to_world(bounds, pixel, 0.0);

        assert_close(cam.zoom, 1.6, 1e-3); // ph/span_h fits the height
        let (_, top) = cam.world_to_pixel(0.0, 0.0);
        let (_, bottom) = cam.world_to_pixel(0.0, 400.0);
        assert_close(top, 0.0, 1e-2);
        assert_close(bottom, pixel.1, 1e-2);
        let (right, _) = cam.world_to_pixel(100.0, 0.0);
        assert!(right <= pixel.0, "the whole graph stays framed");
    }

    #[test]
    fn fit_handles_degenerate_zero_span_world() {
        // Coincident positions: zero span must not produce NaN/inf.
        let cam = GraphCamera::fit_to_world(
            WorldBounds {
                min_x: 3.0,
                min_y: 4.0,
                max_x: 3.0,
                max_y: 4.0,
            },
            (640.0, 400.0),
            8.0,
        );
        assert!(cam.zoom.is_finite() && cam.zoom > 0.0);
        let (px, py) = cam.world_to_pixel(3.0, 4.0);
        assert!(px.is_finite() && py.is_finite());
        assert_close(px, 320.0, 1e-2);
        assert_close(py, 200.0, 1e-2);
    }

    #[test]
    fn world_bounds_from_positions_computes_bbox() {
        let pos = [(1.0, 2.0), (-3.0, 5.0), (7.0, -1.0)];
        let b = WorldBounds::from_positions(&pos);
        assert_eq!(
            b,
            WorldBounds {
                min_x: -3.0,
                min_y: -1.0,
                max_x: 7.0,
                max_y: 5.0
            }
        );
        // Empty input collapses to the zero box; the fit still guards the span.
        assert_eq!(
            WorldBounds::from_positions(&[]),
            WorldBounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0
            }
        );
    }
}
// ---------------------------------------------------------------------------
// Offscreen rasterizer (design.md D2 rounded rects, D3 fontdue labels, D5 f=32)
// ---------------------------------------------------------------------------

use fontdue::{Font, FontSettings};
use std::sync::OnceLock;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PremultipliedColorU8,
    Stroke, Transform,
};

/// RGB color triple in the rasterizer's own space. The `Color → RGB` hop (task
/// 2.2, design D9) happens in [`build_scene`] — the pixel path never hardcodes
/// or converts theme colors.
pub type Rgb = (u8, u8, u8);

/// Canvas cap for runaway pan/zoom: larger sizes are clamped before
/// `Pixmap::new`, so a stretched camera can never exhaust memory (design.md D2
/// "bounded `Pixmap::new`").
const MAX_DIM: u32 = 8192;

/// Bundled OFL font, parsed once (design.md D3/D10: deterministic and
/// headless-safe, no filesystem lookup). A corrupt asset yields `None` for the
/// whole scene so the caller falls back to box drawing.
static FONT: OnceLock<Option<Font>> = OnceLock::new();

fn bundled_font() -> Option<&'static Font> {
    FONT.get_or_init(|| {
        Font::from_bytes(
            include_bytes!("../assets/JetBrainsMono-Regular.ttf").as_slice(),
            FontSettings::default(),
        )
        .ok()
    })
    .as_ref()
}

/// Pixel-space appearance of one graph node: an anti-aliased rounded rect with
/// an optional centered title.
#[derive(Clone)]
pub struct NodeSpec {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
    pub fill: Rgb,
    pub border: Rgb,
    pub border_width: f32,
    pub label: String,
    pub label_color: Rgb,
}

/// Pixel-space appearance of one cable: a quadratic Bézier stroke with a filled
/// direction arrow at `end`.
#[derive(Clone)]
pub struct EdgeSpec {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub ctrl: (f32, f32),
    pub color: Rgb,
    pub width: f32,
}

/// A finished raster pass: opaque RGBA8 (`alpha == 255`, so premultiplied ==
/// straight — design.md D5) ready for the `f=32` kitty transport.
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Rasterize one graph frame: opaque background, cable curves behind
/// rounded-rect nodes, labels on top. Pure over RGB tuples; `None` on
/// degenerate sizes or a broken bundled font.
pub fn render_scene(
    width: u32,
    height: u32,
    background: Rgb,
    nodes: &[NodeSpec],
    edges: &[EdgeSpec],
) -> Option<Scene> {
    let font = bundled_font()?;
    let width = width.min(MAX_DIM);
    let height = height.min(MAX_DIM);
    let mut pixmap = Pixmap::new(width, height)?;
    pixmap.fill(Color::from_rgba8(
        background.0,
        background.1,
        background.2,
        255,
    ));

    for edge in edges {
        draw_edge(&mut pixmap, edge);
    }
    for node in nodes {
        draw_node(&mut pixmap, node, font);
    }

    Some(Scene {
        width,
        height,
        rgba: pixmap.data().to_vec(),
    })
}

/// Point on the circle of radius `r` around `c` at `angle_deg`. Screen y grows
/// downward, so the angles run clockwise on screen — only the resulting point
/// set matters, never the winding.
fn arc_point(c: (f32, f32), r: f32, angle_deg: f32) -> (f32, f32) {
    let a = angle_deg.to_radians();
    (c.0 + r * a.cos(), c.1 + r * a.sin())
}

/// Append one rounded corner as two quadratic Béziers whose control points make
/// each segment pass through its 45° arc midpoint (`C = 2M − (P0+P2)/2`), so
/// the corner tracks the quarter circle. tiny-skia has no `RRect` (design.md
/// D2): a rounded rect is eight `quad_to` calls, two per corner.
fn append_corner(pb: &mut PathBuilder, center: (f32, f32), r: f32, start_deg: f32) {
    for seg in 0..2 {
        let a0 = start_deg + seg as f32 * 45.0;
        let a1 = a0 + 45.0;
        let (p0x, p0y) = arc_point(center, r, a0);
        let (p2x, p2y) = arc_point(center, r, a1);
        let (mx, my) = arc_point(center, r, (a0 + a1) / 2.0);
        pb.quad_to(
            2.0 * mx - (p0x + p2x) / 2.0,
            2.0 * my - (p0y + p2y) / 2.0,
            p2x,
            p2y,
        );
    }
}

/// Anti-aliased rounded-rect outline: `line_to` between corners, two `quad_to`
/// per corner (design.md D2). `None` for degenerate geometry; the radius is
/// clamped to half the smaller side.
fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = radius.clamp(0.0, w.min(h) / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    append_corner(&mut pb, (x + w - r, y + r), r, 270.0);
    pb.line_to(x + w, y + h - r);
    append_corner(&mut pb, (x + w - r, y + h - r), r, 0.0);
    pb.line_to(x + r, y + h);
    append_corner(&mut pb, (x + r, y + h - r), r, 90.0);
    pb.line_to(x, y + r);
    append_corner(&mut pb, (x + r, y + r), r, 180.0);
    pb.close();
    pb.finish()
}

/// Straight-alpha source-over of `src` at coverage `cov`/255 over an opaque
/// `dst`. Alpha stays 255, so premultiplied == straight (design.md D5).
fn blend_over_opaque(dst: PremultipliedColorU8, src: Rgb, cov: u8) -> (u8, u8, u8) {
    let c = cov as u32;
    let inv = 255 - c;
    let bl = |d: u8, s: u8| ((s as u32 * c + d as u32 * inv + 127) / 255) as u8;
    (
        bl(dst.red(), src.0),
        bl(dst.green(), src.1),
        bl(dst.blue(), src.2),
    )
}

/// Rasterize `label` centered inside `node` at a size that scales with the node
/// height. Glyph coverage composites straight-alpha over the opaque canvas;
/// alpha stays 255.
fn draw_label(pixmap: &mut Pixmap, node: &NodeSpec, font: &Font) {
    let px = (node.h * 0.42).clamp(8.0, 40.0);
    let Some(line) = font.horizontal_line_metrics(px) else {
        return;
    };
    let glyphs: Vec<_> = node.label.chars().map(|c| font.rasterize(c, px)).collect();
    let total_w: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
    let mut cursor = node.x + (node.w - total_w) / 2.0;
    // Baseline centered vertically: text spans [baseline + descent, baseline + ascent].
    let baseline = node.y + node.h / 2.0 + (line.ascent + line.descent) / 2.0;
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let pixels = pixmap.pixels_mut();
    for (metrics, coverage) in &glyphs {
        let gx = (cursor + metrics.xmin as f32).round() as i32;
        let gy = (baseline + metrics.ymin as f32).round() as i32;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let x = gx + col as i32;
                let y = gy + row as i32;
                if x < 0 || y < 0 || x >= width || y >= height {
                    continue;
                }
                let cov = coverage[row * metrics.width + col];
                if cov == 0 {
                    continue;
                }
                let idx = (y as u32 * width as u32 + x as u32) as usize;
                let (r, g, b) = blend_over_opaque(pixels[idx], node.label_color, cov);
                // Alpha stays 255 (opaque canvas), so the premultiplied form is
                // always valid; an impossible failure just skips one pixel.
                if let Some(c) = PremultipliedColorU8::from_rgba(r, g, b, 255) {
                    pixels[idx] = c;
                }
            }
        }
        cursor += metrics.advance_width;
    }
}

/// Fill + border a rounded-rect node, then its label on top.
fn draw_node(pixmap: &mut Pixmap, node: &NodeSpec, font: &Font) {
    let Some(path) = rounded_rect_path(node.x, node.y, node.w, node.h, node.radius) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(node.fill.0, node.fill.1, node.fill.2, 255);
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    if node.border_width > 0.0 {
        paint.set_color_rgba8(node.border.0, node.border.1, node.border.2, 255);
        let stroke = Stroke {
            width: node.border_width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    if !node.label.is_empty() {
        draw_label(pixmap, node, font);
    }
}

/// Stroke a quadratic Bézier cable, then fill a direction arrow at `end` along
/// the curve tangent (`B'(1) = 2·(P2 − C)`).
fn draw_edge(pixmap: &mut Pixmap, edge: &EdgeSpec) {
    let mut pb = PathBuilder::new();
    pb.move_to(edge.start.0, edge.start.1);
    pb.quad_to(edge.ctrl.0, edge.ctrl.1, edge.end.0, edge.end.1);
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(edge.color.0, edge.color.1, edge.color.2, 255);
    let stroke = Stroke {
        width: edge.width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);

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
    let base = (edge.end.0 - ux * arrow_len, edge.end.1 - uy * arrow_len);
    let mut pb = PathBuilder::new();
    pb.move_to(edge.end.0, edge.end.1);
    pb.line_to(base.0 + nx * half, base.1 + ny * half);
    pb.line_to(base.0 - nx * half, base.1 - ny * half);
    pb.close();
    if let Some(arrow) = pb.finish() {
        pixmap.fill_path(
            &arrow,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

// ---------------------------------------------------------------------------
// Theme-aware Scene builder (design D9: the `Color → RGB` hop)
// ---------------------------------------------------------------------------

use crate::theme::Theme;
use ratatui::style::Color as ThemeColor;

/// Theme tokens for one rasterized node (design D9): mirror of [`NodeSpec`]
/// whose color fields carry the classified semantic `Color`s from the existing
/// pipeline (error red > diff > latency ramp > cable kind) instead of triples.
#[derive(Clone)]
pub struct NodeTokenSpec {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
    pub fill: ThemeColor,
    pub border: ThemeColor,
    pub border_width: f32,
    pub label: String,
    pub label_color: ThemeColor,
}

/// Theme tokens for one cable (design D9): mirror of [`EdgeSpec`] carrying the
/// classified edge `Color` (error red > diff > latency ramp > cable kind).
#[derive(Clone)]
pub struct EdgeTokenSpec {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub ctrl: (f32, f32),
    pub color: ThemeColor,
    pub width: f32,
}

/// Build a raster scene from classified theme tokens (design D9): the single
/// `Color → RGB` hop applied before anything touches the pixel path. Geometry
/// (`x/y/w/h`, curve control points, widths) comes from the caller; only the
/// tokens are resolved here, so `ui.rs` reuses its classification pipeline
/// verbatim and the rasterizer stays RGB-pure. Returns `None` on degenerate
/// sizes or a broken bundled font (same contract as [`render_scene`]).
pub fn build_scene(
    theme: &Theme,
    width: u32,
    height: u32,
    background: ThemeColor,
    nodes: &[NodeTokenSpec],
    edges: &[EdgeTokenSpec],
) -> Option<Scene> {
    let nodes: Vec<NodeSpec> = nodes
        .iter()
        .map(|n| NodeSpec {
            x: n.x,
            y: n.y,
            w: n.w,
            h: n.h,
            radius: n.radius,
            fill: theme.rgb(n.fill),
            border: theme.rgb(n.border),
            border_width: n.border_width,
            label: n.label.clone(),
            label_color: theme.rgb(n.label_color),
        })
        .collect();
    let edges: Vec<EdgeSpec> = edges
        .iter()
        .map(|e| EdgeSpec {
            start: e.start,
            end: e.end,
            ctrl: e.ctrl,
            color: theme.rgb(e.color),
            width: e.width,
        })
        .collect();
    render_scene(width, height, theme.rgb(background), &nodes, &edges)
}

#[cfg(test)]
mod rasterizer_tests {
    use super::*;

    const BG: Rgb = (20, 20, 20);

    fn pixel(scene: &Scene, x: u32, y: u32) -> (u8, u8, u8) {
        let i = ((y * scene.width + x) * 4) as usize;
        (scene.rgba[i], scene.rgba[i + 1], scene.rgba[i + 2])
    }

    fn all_opaque(scene: &Scene) -> bool {
        scene.rgba.chunks_exact(4).all(|p| p[3] == 255)
    }

    fn non_background_count(scene: &Scene) -> usize {
        scene
            .rgba
            .chunks_exact(4)
            .filter(|p| p[0] != BG.0 || p[1] != BG.1 || p[2] != BG.2)
            .count()
    }

    fn sample_node() -> NodeSpec {
        NodeSpec {
            x: 30.0,
            y: 30.0,
            w: 80.0,
            h: 40.0,
            radius: 8.0,
            fill: (60, 60, 80),
            border: (200, 200, 200),
            border_width: 2.0,
            label: "OUT".to_string(),
            label_color: (240, 240, 200),
        }
    }

    /// Curve stays below the node (node ends at y=70; the quad's minimum is
    /// y=80 at its midpoint) and inside the 200×160 canvas.
    fn sample_edge() -> EdgeSpec {
        EdgeSpec {
            start: (30.0, 140.0),
            ctrl: (100.0, 20.0),
            end: (170.0, 140.0),
            color: (100, 220, 140),
            width: 3.0,
        }
    }

    #[test]
    fn render_small_node_and_edge_produces_non_blank_pixels() {
        let scene = render_scene(200, 160, BG, &[sample_node()], &[sample_edge()])
            .expect("bounded scene renders");
        assert!(
            all_opaque(&scene),
            "opaque canvas: f=32 premultiplied == straight"
        );
        // Node interior is fully covered by the fill.
        assert_eq!(pixel(&scene, 50, 50), sample_node().fill);
        // The edge midpoint lies on the quadratic curve (B(0.5) = (100, 80)).
        assert_ne!(pixel(&scene, 100, 80), BG);
        // The direction-arrow tip at the edge end is painted.
        assert_ne!(pixel(&scene, 170, 140), BG);
        // Node body + label + cable + arrow together produce a large
        // non-background region.
        assert!(non_background_count(&scene) > 1000);
    }

    #[test]
    fn rounded_corner_has_anti_aliased_coverage() {
        let node = NodeSpec {
            x: 10.0,
            y: 10.0,
            w: 60.0,
            h: 30.0,
            radius: 8.0,
            label: String::new(),
            border_width: 0.0,
            ..sample_node()
        };
        let scene = render_scene(120, 80, BG, std::slice::from_ref(&node), &[]).expect("renders");
        assert!(all_opaque(&scene));
        // Scan the top-left corner square (arc center (18,18), r=8): the AA
        // edge must contain a pixel strictly between background and fill
        // (coverage in (0,1)) alongside fully-off and fully-on pixels. On an
        // opaque canvas coverage shows up in RGB, not alpha.
        let mut blends = 0;
        let mut on = 0;
        let mut off = 0;
        for y in 10..18 {
            for x in 10..18 {
                let p = pixel(&scene, x, y);
                if p.0 > BG.0
                    && p.0 < node.fill.0
                    && p.1 > BG.1
                    && p.1 < node.fill.1
                    && p.2 > BG.2
                    && p.2 < node.fill.2
                {
                    blends += 1;
                } else if p == node.fill {
                    on += 1;
                } else if p == BG {
                    off += 1;
                }
            }
        }
        assert!(blends > 0, "no anti-aliased corner pixel found");
        assert!(on > 0, "corner square must contain fully-covered fill");
        assert!(off > 0, "corner square must contain uncovered background");
    }

    #[test]
    fn label_glyphs_render_over_opaque_background() {
        // Taller node → px 23.5, so glyph coverage is unambiguous.
        let node = NodeSpec {
            h: 56.0,
            ..sample_node()
        };
        let scene = render_scene(200, 120, BG, std::slice::from_ref(&node), &[]).expect("renders");
        assert!(all_opaque(&scene));
        // Glyph interiors at full coverage composite to exactly the label color;
        // nothing else in the scene uses it, so this proves legible text.
        let label_pixels = scene
            .rgba
            .chunks_exact(4)
            .filter(|p| (p[0], p[1], p[2]) == node.label_color)
            .count();
        assert!(
            label_pixels >= 30,
            "legible label: only {label_pixels} glyph pixels"
        );
        // Every label pixel stays inside the node's rect.
        for y in 0..scene.height {
            for x in 0..scene.width {
                if pixel(&scene, x, y) == node.label_color {
                    assert!(x >= node.x as u32 && x < (node.x + node.w) as u32);
                    assert!(y >= node.y as u32 && y < (node.y + node.h) as u32);
                }
            }
        }
    }

    #[test]
    fn build_scene_converts_theme_tokens_via_rgb_hop() {
        // Task 2.2 verification: the Scene builder is the `Color → RGB` hop —
        // tokens in, RGB pixels out — and `render_scene` never sees a token.
        use crate::theme::Theme;
        let theme = Theme::classic();
        let nodes = [NodeTokenSpec {
            x: 30.0,
            y: 30.0,
            w: 80.0,
            h: 40.0,
            radius: 8.0,
            fill: theme.graph_node_dim,      // Gray
            border: theme.graph_node_border, // White
            border_width: 2.0,
            label: "OUT".to_string(),
            label_color: theme.graph_node_title, // Yellow
        }];
        let edges = [EdgeTokenSpec {
            start: (30.0, 140.0),
            ctrl: (100.0, 20.0),
            end: (170.0, 140.0),
            color: theme.graph_edge_error, // Red
            width: 3.0,
        }];
        let scene =
            build_scene(&theme, 200, 160, theme.status_bg, &nodes, &edges).expect("renders");
        assert!(all_opaque(&scene));
        // The node fill hit the theme-resolved triple — proof the hop ran.
        assert_eq!(pixel(&scene, 50, 50), theme.rgb(theme.graph_node_dim));
        // The error-red curve is painted over the background.
        assert_ne!(pixel(&scene, 100, 80), theme.rgb(theme.status_bg));
    }

    #[test]
    fn scene_length_and_opacity_match_declared_dimensions() {
        // Task 3.2: the RGBA buffer is exactly w×h×4 bytes and fully opaque
        // (f=32 premultiplied == straight), and pixels away from any content
        // stay at the painted background.
        let (w, h) = (200u32, 160u32);
        let scene =
            render_scene(w, h, BG, &[sample_node()], &[sample_edge()]).expect("bounded scene");
        assert_eq!(scene.rgba.len(), (w * h * 4) as usize);
        assert!(all_opaque(&scene));
        // Top-left corner (5,5) is clear of the node (x 30..110, y 30..70) and
        // the curve (which stays below y≈80): the background must show through.
        assert_eq!(pixel(&scene, 5, 5), BG);
    }

    #[test]
    fn degenerate_dimensions_return_none() {
        // Task 3.2: a zero-width or zero-height canvas has no Pixmap; the
        // caller falls back to box drawing instead of receiving garbage.
        assert!(render_scene(0, 160, BG, &[], &[]).is_none());
        assert!(render_scene(200, 0, BG, &[], &[]).is_none());
        assert!(render_scene(0, 0, BG, &[], &[]).is_none());
    }

    #[test]
    fn huge_dimensions_clamp_to_max_dim() {
        // Task 3.2: runaway pan/zoom must never reach `Pixmap::new` unchecked;
        // larger requests clamp to MAX_DIM on each axis (design D2).
        let wide = render_scene(2 * MAX_DIM, 10, BG, &[], &[]).expect("clamped wide canvas");
        assert_eq!(wide.width, MAX_DIM);
        assert_eq!(wide.height, 10);
        assert_eq!(wide.rgba.len(), (MAX_DIM * 10 * 4) as usize);
        let tall = render_scene(10, 2 * MAX_DIM, BG, &[], &[]).expect("clamped tall canvas");
        assert_eq!(tall.width, 10);
        assert_eq!(tall.height, MAX_DIM);
        assert_eq!(tall.rgba.len(), (10 * MAX_DIM * 4) as usize);
    }

    #[test]
    fn identical_inputs_render_byte_identical_scenes() {
        // Task 3.2: the rasterizer is deterministic — two runs with identical
        // inputs produce byte-identical buffers (same frames do not flicker).
        let nodes = [sample_node()];
        let edges = [sample_edge()];
        let a = render_scene(200, 160, BG, &nodes, &edges).expect("first run");
        let b = render_scene(200, 160, BG, &nodes, &edges).expect("second run");
        assert_eq!(a.width, b.width);
        assert_eq!(a.height, b.height);
        assert_eq!(a.rgba, b.rgba);
    }
}
