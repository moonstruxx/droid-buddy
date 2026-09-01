//! Pixel-space camera and (later) offscreen rasterizer for the graph surface.
//!
//! This module is intentionally pure: no terminal, no ratatui, no `App` — only
//! `f32` math over the layout solver's world plane and the kitty image's pixel
//! plane. Task 2.1 wires it into `render_graph` by replacing the bounding-box
//! fit with the camera and deriving `graph_node_rects` through the inverse.

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

    /// Initial fit: zoom/pan so `world_bounds` centers in `pixel_size` while a
    /// node spanning one world unit stays at least `min_node_px` pixels wide.
    /// The fit zoom (`min(pw/span_w, ph/span_h)`) is clamped up to
    /// `min_node_px` pixels per world unit, so a spread-out world overflows the
    /// viewport instead of collapsing nodes below the legible minimum — the
    /// user pans/zooms from there. `min_node_px = 0` degrades to a pure fit.
    pub fn fit_to_world(bounds: WorldBounds, pixel_size: (f32, f32), min_node_px: f32) -> Self {
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
