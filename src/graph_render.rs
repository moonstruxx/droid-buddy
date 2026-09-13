//! Pixel-space camera and backend-neutral scene spec for the graph surface.
//!
//! This module is intentionally pure: no window, no `App` — only `f32` math
//! over the layout solver's world plane and the pixel plane, plus the neutral
//! [`SceneSpec`] (design D3) the egui window painter consumes under the same
//! [`GraphCamera`]: node frames with circuit identity and port markers,
//! per-cable colored Bézier edges with resolved precedence state, cluster
//! containers, labels, resolved RGB colors. Below the `Color → RGB` hop
//! everything is RGB-pure.

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
/// are the graph window's egui points.
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
    /// the height with the whole graph framed. Framing wins over legibility:
    /// the `min_node_px` floor applies only when the floored zoom still frames
    /// the whole world — otherwise the pure fit wins, so a large patch never
    /// over-zooms to a single circuit (bug droid_tui-ttz: a 2.2 floor on a
    /// ~0.1 fit showed one node and the `+`/`-` presets could not reach the
    /// fit). Zoom out/in from the fitted camera for detail.
    /// `min_node_px = 0` degrades to a pure fit in both cases.
    pub fn fit_to_world(bounds: WorldBounds, pixel_size: (f32, f32), min_node_px: f32) -> Self {
        let pixel_size = (pixel_size.0.max(1.0), pixel_size.1.max(1.0));
        let span_w = (bounds.max_x - bounds.min_x).max(MIN_SPAN);
        let span_h = (bounds.max_y - bounds.min_y).max(MIN_SPAN);
        let fit_zoom_w = pixel_size.0 / span_w;
        let fit_zoom_h = pixel_size.1 / span_h;
        let fit_zoom = fit_zoom_w.min(fit_zoom_h);
        // The floor must still frame the graph: it binds only when the floored
        // world fits the canvas on both axes (i.e. it is already ≤ the fit and
        // hence a no-op raise). A floor above the fit would push nodes
        // off-canvas, so the pure fit wins and every corner stays in view.
        let floor_frames = min_node_px > 0.0
            && span_w * min_node_px <= pixel_size.0
            && span_h * min_node_px <= pixel_size.1;
        let zoom = if floor_frames {
            fit_zoom.max(min_node_px).max(MIN_ZOOM)
        } else {
            fit_zoom.max(MIN_ZOOM)
        };
        // When the min-node clamp overrides the fit, the world overflows the
        // canvas; anchor the pan on the world's top-left corner so the initial
        // view shows the graph's start instead of empty space around its
        // center (a vertical chain's center is a gap between nodes).
        let pan = if zoom > fit_zoom {
            (bounds.min_x * zoom, bounds.min_y * zoom)
        } else {
            let (cx, cy) = (
                (bounds.min_x + bounds.max_x) / 2.0,
                (bounds.min_y + bounds.max_y) / 2.0,
            );
            (
                cx * zoom - pixel_size.0 / 2.0,
                cy * zoom - pixel_size.1 / 2.0,
            )
        };
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
    fn fit_to_world_frames_a_spread_world_instead_of_clamping() {
        // Bug droid_tui-ttz: a spread-out world (1000 nodes one world unit
        // apart) with a legibility floor far above the fit over-zoomed to a
        // single circuit. Framing wins: the pure fit applies and every corner
        // of the world stays inside the viewport.
        let positions: Vec<(f32, f32)> = (0..1000).map(|i| (i as f32, (i % 50) as f32)).collect();
        let bounds = WorldBounds::from_positions(&positions);
        let pixel = (640.0, 400.0);
        let min_node_px = 20.0;
        let cam = GraphCamera::fit_to_world(bounds, pixel, min_node_px);

        // The width-bound fit wins over the floor: zoom = pw/span_w.
        let span_w = (bounds.max_x - bounds.min_x).max(MIN_SPAN);
        let span_h = (bounds.max_y - bounds.min_y).max(MIN_SPAN);
        let fit_zoom = (pixel.0 / span_w).min(pixel.1 / span_h);
        assert_close(cam.zoom, fit_zoom, 1e-3);
        assert!(
            cam.zoom < min_node_px,
            "floor must not break a fit that needs to be smaller"
        );
        // Every corner of the world bounds stays inside the viewport.
        let (left, top) = cam.world_to_pixel(bounds.min_x, bounds.min_y);
        let (right, bottom) = cam.world_to_pixel(bounds.max_x, bounds.max_y);
        assert!(left >= 0.0 && top >= 0.0, "top-left corner off-canvas");
        assert!(
            right <= pixel.0 && bottom <= pixel.1,
            "bottom-right corner off-canvas: ({right},{bottom}) vs ({},{})",
            pixel.0,
            pixel.1
        );
    }

    #[test]
    fn fit_centers_content_when_fit_zoom_is_not_clamped() {
        // A world that fits at the fit zoom (no min-node clamp override) keeps
        // the center anchor: the world-bounds center lands on the viewport
        // center.
        let bounds = WorldBounds {
            min_x: -10.0,
            min_y: -5.0,
            max_x: 30.0,
            max_y: 15.0,
        };
        let pixel = (640.0, 400.0);
        let cam = GraphCamera::fit_to_world(bounds, pixel, 4.0);
        // fit_zoom = min(640/40, 400/20) = 16 ≥ 4 → not clamped.
        assert_close(cam.zoom, 16.0, 1e-3);
        let (cx, cy) = (
            (bounds.min_x + bounds.max_x) / 2.0,
            (bounds.min_y + bounds.max_y) / 2.0,
        );
        let (px, py) = cam.world_to_pixel(cx, cy);
        assert_close(px, pixel.0 / 2.0, 1e-2);
        assert_close(py, pixel.1 / 2.0, 1e-2);
    }

    #[test]
    fn fit_frames_tall_world_when_height_binds() {
        // Regression: a real patch's solver output is a vertical chain (tall,
        // narrow world). The height-bound fit wins over the min-node floor so
        // the whole graph stays framed (the graph surface rendered blank when
        // the clamp pushed the tall world off-canvas).
        let bounds = WorldBounds {
            min_x: -77.0,
            min_y: 3.0,
            max_x: 97.0,
            max_y: 709.0,
        };
        let pixel = (624.0, 304.0); // 100x24 main area minus one node frame
        let min_node_px = 2.2;
        let cam = GraphCamera::fit_to_world(bounds, pixel, min_node_px);
        // The height-bound fit wins over the floor: zoom = ph/span_h.
        assert_close(cam.zoom, 304.0 / 706.0, 1e-3);
        assert!(
            cam.zoom < min_node_px,
            "floor must not break a height-bound fit"
        );
        // Every corner of the world bounds stays inside the viewport.
        let (left, top) = cam.world_to_pixel(bounds.min_x, bounds.min_y);
        let (right, bottom) = cam.world_to_pixel(bounds.max_x, bounds.max_y);
        assert!(left >= 0.0 && top >= 0.0, "top-left corner off-canvas");
        assert!(
            right <= pixel.0 && bottom <= pixel.1,
            "bottom-right corner off-canvas: ({right},{bottom}) vs ({},{})",
            pixel.0,
            pixel.1
        );
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
        // aligned with the pixels the window painter draws.
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
    fn fit_frames_a_tall_graph_even_with_a_legibility_floor() {
        // A tall fan-out graph (like arpeggio1: many buttons stacked vertically
        // feeding one circuit) must stay framed even when `min_node_px` is set.
        // The floor applies only when the width constraint binds; here the
        // height binds, so the pure fit wins and no node is pushed off-canvas.
        let bounds = WorldBounds {
            min_x: -76.0,
            min_y: 2.6,
            max_x: 97.0,
            max_y: 700.0,
        };
        let pixel = (784.0, 464.0);
        let min_node_px = 2.2;
        let cam = GraphCamera::fit_to_world(bounds, pixel, min_node_px);

        // The height-bound fit wins over the floor: zoom = ph/span_h.
        assert_close(cam.zoom, 464.0 / 697.4, 1e-3);
        assert!(
            cam.zoom < min_node_px,
            "floor must not break a height-bound fit"
        );
        // Every corner of the world bounds stays inside the viewport.
        let (left, top) = cam.world_to_pixel(bounds.min_x, bounds.min_y);
        let (right, bottom) = cam.world_to_pixel(bounds.max_x, bounds.max_y);
        assert!(left >= 0.0 && top >= 0.0, "top-left corner off-canvas");
        assert!(
            right <= pixel.0 && bottom <= pixel.1,
            "bottom-right corner off-canvas: ({right},{bottom}) vs ({},{})",
            pixel.0,
            pixel.1
        );
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
// Backend-neutral scene spec (design D3): the description the egui window
// painter consumes — only plain tuples, `f32`, and `String`.
// ---------------------------------------------------------------------------

/// RGB color triple in the neutral scene spec. The `Color → RGB` hop (design
/// D9) resolves theme tokens before any painter touches the spec.
pub type Rgb = (u8, u8, u8);

/// Cable-kind classification carried on the spec (design D5): produced by
/// [`CableKind::from_circuit`]'s inference over the leading circuit so the
/// window painter can reproduce kind-colored cables and legends. Both the
/// scene builder and the painter consume this enum directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CableKind {
    Control,
    Audio,
    Midi,
    #[default]
    Unknown,
}

/// Diff-overlay state of one edge, when the diff is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDiffState {
    Added,
    Removed,
    Changed,
}

/// Latency state of one edge: the ramp stop index (0–4, cold→hot) and whether
/// the edge is a back edge. The resolved [`EdgeSpec`] color already encodes
/// the ramp; the state lets the window painter draw the latency legend and
/// tooltip readouts without re-deriving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EdgeLatency {
    pub ramp_stop: usize,
    pub back_edge: bool,
}

/// Pixel-space appearance of one graph node: an anti-aliased rounded rect with
/// an optional centered title, plus the circuit identity and port presence the
/// window painter needs for selection propagation (`x`/`p`/`e`, task 3.1/3.3)
/// and port markers. The scene builder (`crate::gui::build_scene_spec`)
/// fills the identity fields per node.
#[derive(Debug, Clone, PartialEq)]
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
    /// Circuit/section name of the node (selection and disable targeting).
    pub circuit: String,
    /// Zero-based occurrence index among same-named circuits.
    pub instance_index: usize,
    /// Left input port marker: the node sinks at least one cable.
    pub input_port: bool,
    /// Right output port marker: the node sources at least one cable.
    pub output_port: bool,
}

/// Pixel-space appearance of one cable: a quadratic Bézier stroke with a
/// filled direction arrow at `end`, plus the resolved semantic state (design
/// D5). `color` is the final winner of the precedence chain (error red, then
/// diff, then latency ramp, then cable kind, then dim) resolved to RGB; the
/// state fields let the window painter reproduce legends, tooltips, and
/// per-state styling without consulting graph.rs. The scene builder resolves
/// the precedence chain; the resolved color travels in `color`.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeSpec {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub ctrl: (f32, f32),
    pub color: Rgb,
    pub width: f32,
    /// Inferred cable kind of the producing circuit.
    pub kind: CableKind,
    /// A topology-validation or wiring-outlier finding references the cable.
    pub error: bool,
    /// An incident circuit is disabled (`graph_edge_dim` won the chain).
    pub dim: bool,
    /// Diff-overlay state when the diff is showing.
    pub diff: Option<EdgeDiffState>,
    /// Latency-ramp state when latency coloring is on.
    pub latency: Option<EdgeLatency>,
}

/// Pixel-space cluster container (design D5): a titled bordered box enclosing
/// its member node rects, a padded union the scene builder computes.
/// `member_indices` index into [`SceneSpec::nodes`] so the
/// window painter can map the container back to node identity for diff tinting
/// (all-added/all-removed members) and hit-testing.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterSpec {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub title: String,
    pub border: Rgb,
    pub title_color: Rgb,
    pub member_indices: Vec<usize>,
}

/// A fully resolved, backend-neutral graph frame: opaque background plus node,
/// cable, and cluster-container appearance in RGB space. The egui window
/// painter (task 2.2) draws this spec under the same [`GraphCamera`].
#[derive(Debug, Clone, PartialEq)]
pub struct SceneSpec {
    pub background: Rgb,
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
    pub clusters: Vec<ClusterSpec>,
}

/// How `CableKind::from_circuit` maps tokens and schema designations to
impl CableKind {
    /// Classify a producing circuit's output: the schema's declared
    /// `cable_kind` designation wins (plugin-added circuits may opt out of
    /// inference), then clock/gate/trigger/pulsar/div emit control signals;
    /// midi/note/seq/pitch emit musical/midi signals; anything else is
    /// treated as audio/CV.
    pub fn from_circuit(circuit: &str) -> CableKind {
        match crate::schema::load_schema()
            .circuits
            .get(&circuit.to_ascii_lowercase())
            .and_then(|def| def.cable_kind.as_deref())
        {
            Some("control") => return Self::Control,
            Some("midi") => return Self::Midi,
            _ => {}
        }
        let name = circuit.to_ascii_lowercase();
        if ["clock", "gate", "trigger", "pulsar", "div"]
            .iter()
            .any(|k| name.contains(k))
        {
            Self::Control
        } else if ["midi", "note", "seq", "pitch"]
            .iter()
            .any(|k| name.contains(k))
        {
            Self::Midi
        } else {
            Self::Audio
        }
    }
}

/// The producing circuit of a cable: the source end of the first edge carrying
/// it, resolved to a node's circuit name.
pub fn cable_source_circuit<'a>(graph: &'a crate::graph::Graph, cable: &str) -> Option<&'a str> {
    let source = graph
        .edges
        .iter()
        .find(|e| e.cable == cable)?
        .source
        .clone();
    graph
        .nodes
        .iter()
        .find(|n| n.id == source)
        .map(|n| n.circuit.as_str())
}

/// Cable kind inferred from its producing circuit; `Unknown` when no edge
/// produces it.
pub fn cable_kind(graph: &crate::graph::Graph, cable: &str) -> CableKind {
    match cable_source_circuit(graph, cable) {
        Some(circuit) => CableKind::from_circuit(circuit),
        None => CableKind::Unknown,
    }
}
