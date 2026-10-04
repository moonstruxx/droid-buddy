//! Side-bound label placement model for the performance view
//! (`performance-view` change, task 1.2).
//!
//! Pure module: plain structs + `f32` math, no egui/winit dependency
//! (mirrors the conventions of `src/physical.rs`). The performance view
//! renders the rack compact and centered while every element label renders
//! big in the freed field around it, joined to its host cell by a leader
//! line. Labels never enter the rack rect.
//!
//! Placement is greedy + side-bound (design: label placement is NP-hard in
//! general; greedy with a stable side binding is legible and testable):
//!
//! 1. each label is assigned a side ([`Side`]) by host-cell quadrant relative
//!    to the rack center (dominant axis wins);
//! 2. labels within a side are ordered by host coordinate along that side
//!    (host id breaks ties), which minimizes leader-line crossings;
//! 3. labels stack along their side in bands running outward from the rack
//!    edge, wrapping to a new band when a band overflows the side length, so
//!    no two label rects overlap;
//! 4. each label gets a leader-line anchor: the host-cell edge point facing
//!    its assigned side.
//!
//! Placement is deterministic: the same input always yields the same output,
//! regardless of input order (inputs are sorted by host id up front and the
//! output is emitted in host-id order).

// ---------------------------------------------------------------------------
// Geometry primitives
// ---------------------------------------------------------------------------

/// Axis-aligned rectangle in screen space (`y` grows downward).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Center point of the rect.
    pub fn center(self) -> Point {
        Point {
            x: self.x + self.w * 0.5,
            y: self.y + self.h * 0.5,
        }
    }

    /// True when the interiors overlap (edge-touching counts as disjoint).
    pub fn overlaps(self, other: Rect) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

/// 2D point in screen space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// ---------------------------------------------------------------------------
// Placement model
// ---------------------------------------------------------------------------

/// Which side of the rack a label is bound to. The binding is stable across
/// frames for a fixed rack + host layout: it depends only on the host-cell
/// quadrant relative to the rack center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Over,
    Under,
    Left,
    Right,
}

/// One label to place: the host cell it annotates plus its measured size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelInput {
    /// Opaque host id (e.g. the index into `patch.hw_components`); used for
    /// deterministic ordering and echoed back on the output.
    pub host_id: usize,
    /// Host cell rect in the same space as the rack rect.
    pub host_rect: Rect,
    /// Measured label size (width, height).
    pub label_w: f32,
    pub label_h: f32,
}

/// Placed label: side binding, label rect outside the rack rect, and the
/// leader-line anchor on the host cell edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedLabel {
    pub host_id: usize,
    pub side: Side,
    /// Label rect; disjoint from the rack rect and from every other label.
    pub label_rect: Rect,
    /// Leader-line anchor: host-cell edge point facing `side`.
    pub anchor: Point,
}

/// Gap between the rack edge and the first label band, in screen units.
pub const SIDE_MARGIN: f32 = 4.0;
/// Gap between adjacent labels (along a band) and between bands.
pub const LABEL_GAP: f32 = 2.0;

/// Assign sides and stack every label. See the module docs for the algorithm.
/// Returns one [`PlacedLabel`] per input, sorted by `host_id`.
pub fn place_labels(rack: Rect, inputs: &[LabelInput]) -> Vec<PlacedLabel> {
    // Sort up front so the output is stable regardless of input order.
    let mut sorted: Vec<LabelInput> = inputs.to_vec();
    sorted.sort_by_key(|input| input.host_id);

    let center = rack.center();
    // Bucket host ids per side, in host-coordinate order (host id tiebreak).
    let mut over: Vec<usize> = Vec::new();
    let mut under: Vec<usize> = Vec::new();
    let mut left: Vec<usize> = Vec::new();
    let mut right: Vec<usize> = Vec::new();
    for input in &sorted {
        let host = input.host_rect.center();
        let dx = host.x - center.x;
        let dy = host.y - center.y;
        // Dominant axis wins; ties fall to the horizontal sides. `y` grows
        // downward, so positive `dy` is below the center.
        if dx.abs() >= dy.abs() {
            if dx >= 0.0 {
                right.push(input.host_id);
            } else {
                left.push(input.host_id);
            }
        } else if dy >= 0.0 {
            under.push(input.host_id);
        } else {
            over.push(input.host_id);
        }
    }

    // Order within a side by host coordinate along that side (host id
    // tiebreak keeps it total); this minimizes leader-line crossings.
    let by_id = |a: &usize, b: &usize| a.cmp(b);
    over.sort_by(|a, b| {
        let ha = sorted.iter().find(|i| i.host_id == *a).unwrap();
        let hb = sorted.iter().find(|i| i.host_id == *b).unwrap();
        ha.host_rect
            .center()
            .x
            .partial_cmp(&hb.host_rect.center().x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| by_id(a, b))
    });
    under.sort_by(|a, b| {
        let ha = sorted.iter().find(|i| i.host_id == *a).unwrap();
        let hb = sorted.iter().find(|i| i.host_id == *b).unwrap();
        ha.host_rect
            .center()
            .x
            .partial_cmp(&hb.host_rect.center().x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| by_id(a, b))
    });
    left.sort_by(|a, b| {
        let ha = sorted.iter().find(|i| i.host_id == *a).unwrap();
        let hb = sorted.iter().find(|i| i.host_id == *b).unwrap();
        ha.host_rect
            .center()
            .y
            .partial_cmp(&hb.host_rect.center().y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| by_id(a, b))
    });
    right.sort_by(|a, b| {
        let ha = sorted.iter().find(|i| i.host_id == *a).unwrap();
        let hb = sorted.iter().find(|i| i.host_id == *b).unwrap();
        ha.host_rect
            .center()
            .y
            .partial_cmp(&hb.host_rect.center().y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| by_id(a, b))
    });

    let lookup = |id: usize| *sorted.iter().find(|i| i.host_id == id).unwrap();
    let mut placed: Vec<PlacedLabel> = Vec::with_capacity(sorted.len());
    // Horizontal sides stack bands vertically outward from the rack edge;
    // vertical sides stack bands horizontally outward.
    stack_side(rack, &over, Side::Over, true, &lookup, &mut placed);
    stack_side(rack, &under, Side::Under, true, &lookup, &mut placed);
    stack_side(rack, &left, Side::Left, false, &lookup, &mut placed);
    stack_side(rack, &right, Side::Right, false, &lookup, &mut placed);

    placed.sort_by_key(|p| p.host_id);
    placed
}

/// Stack one side's labels in bands running outward from the rack edge.
///
/// When `horizontal` the side is Over/Under: labels run along `x` inside the
/// rack's `x` span and bands step outward in `y`. Otherwise the side is
/// Left/Right: labels run along `y` inside the rack's `y` span and bands
/// step outward in `x`. A band wraps when the next label would overflow the
/// side length; an oversized single label still gets its own band.
fn stack_side(
    rack: Rect,
    ids: &[usize],
    side: Side,
    horizontal: bool,
    lookup: &dyn Fn(usize) -> LabelInput,
    out: &mut Vec<PlacedLabel>,
) {
    // Outward sign along the band-step axis: Over/Left step negative,
    // Under/Right step positive.
    let outward: f32 = match side {
        Side::Over | Side::Left => -1.0,
        Side::Under | Side::Right => 1.0,
    };
    // Rack edge the first band sits against + side length along the run axis.
    let (edge, side_len) = if horizontal {
        let edge = match side {
            Side::Over => rack.y,
            _ => rack.y + rack.h,
        };
        (edge, rack.w)
    } else {
        let edge = match side {
            Side::Left => rack.x,
            _ => rack.x + rack.w,
        };
        (edge, rack.h)
    };
    // Rack span start along the run axis (bands are left/top-aligned to it).
    let span_start = if horizontal { rack.x } else { rack.y };

    let mut cursor = 0.0_f32; // run position inside the current band
    let mut band_out = SIDE_MARGIN; // outward offset of the current band
    let mut band_thickness = 0.0_f32; // max cross size seen in this band
    let mut band_started = false;

    for id in ids {
        let input = lookup(*id);
        let (run, cross) = if horizontal {
            (input.label_w, input.label_h)
        } else {
            (input.label_h, input.label_w)
        };
        // Wrap when the label would overflow the side length (but never
        // wrap an empty band — an oversized label owns its band).
        if band_started && cursor + run > side_len + f32::EPSILON {
            band_out += band_thickness + LABEL_GAP;
            cursor = 0.0;
            band_thickness = 0.0;
            // No `band_started = false`: the current label is placed into the
            // fresh band immediately below, so the band is non-empty.
        }
        let label_rect = if horizontal {
            let y = if outward < 0.0 {
                edge - band_out - cross
            } else {
                edge + band_out
            };
            Rect::new(span_start + cursor, y, input.label_w, input.label_h)
        } else {
            let x = if outward < 0.0 {
                edge - band_out - cross
            } else {
                edge + band_out
            };
            Rect::new(x, span_start + cursor, input.label_w, input.label_h)
        };
        cursor += run + LABEL_GAP;
        band_thickness = band_thickness.max(cross);
        band_started = true;

        out.push(PlacedLabel {
            host_id: *id,
            side,
            label_rect,
            anchor: edge_anchor(input.host_rect, side),
        });
    }
}

/// Leader-line anchor: the host-cell edge midpoint facing `side`.
fn edge_anchor(host: Rect, side: Side) -> Point {
    let c = host.center();
    match side {
        Side::Over => Point { x: c.x, y: host.y },
        Side::Under => Point {
            x: c.x,
            y: host.y + host.h,
        },
        Side::Left => Point { x: host.x, y: c.y },
        Side::Right => Point {
            x: host.x + host.w,
            y: c.y,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rack() -> Rect {
        Rect::new(100.0, 100.0, 200.0, 200.0) // center (200, 200)
    }

    fn input(host_id: usize, hx: f32, hy: f32) -> LabelInput {
        LabelInput {
            host_id,
            host_rect: Rect::new(hx, hy, 10.0, 10.0),
            label_w: 40.0,
            label_h: 12.0,
        }
    }

    #[test]
    fn side_assignment_follows_host_quadrant() {
        let placed = place_labels(
            rack(),
            &[
                input(0, 195.0, 110.0), // top center -> Over
                input(1, 195.0, 280.0), // bottom center -> Under
                input(2, 110.0, 195.0), // middle left -> Left
                input(3, 280.0, 195.0), // middle right -> Right
            ],
        );
        let side_of = |id: usize| placed.iter().find(|p| p.host_id == id).unwrap().side;
        assert_eq!(side_of(0), Side::Over);
        assert_eq!(side_of(1), Side::Under);
        assert_eq!(side_of(2), Side::Left);
        assert_eq!(side_of(3), Side::Right);
    }

    #[test]
    fn dominant_axis_wins_on_diagonal() {
        // Host up-right but mostly right -> Right; host up-right but mostly
        // up -> Over.
        let placed = place_labels(
            rack(),
            &[
                input(0, 260.0, 160.0), // dx=+65, dy=-35 -> Right
                input(1, 230.0, 110.0), // dx=+35, dy=-85 -> Over
            ],
        );
        let side_of = |id: usize| placed.iter().find(|p| p.host_id == id).unwrap().side;
        assert_eq!(side_of(0), Side::Right);
        assert_eq!(side_of(1), Side::Over);
    }

    #[test]
    fn placement_is_stable_regardless_of_input_order() {
        let forward = place_labels(
            rack(),
            &[
                input(0, 195.0, 110.0),
                input(1, 195.0, 280.0),
                input(2, 110.0, 195.0),
                input(3, 280.0, 195.0),
            ],
        );
        let reversed = place_labels(
            rack(),
            &[
                input(3, 280.0, 195.0),
                input(2, 110.0, 195.0),
                input(1, 195.0, 280.0),
                input(0, 195.0, 110.0),
            ],
        );
        assert_eq!(forward, reversed);
        // Output is emitted in host-id order.
        let ids: Vec<usize> = forward.iter().map(|p| p.host_id).collect();
        assert_eq!(ids, vec![0, 1, 2, 3]);
    }

    #[test]
    fn label_rects_avoid_rack_and_each_other() {
        // 24 labels spread over all four sides, several bands per side.
        let mut inputs = Vec::new();
        for i in 0..6 {
            let f = i as f32;
            inputs.push(input(i, 120.0 + f * 30.0, 110.0)); // top row
            inputs.push(input(10 + i, 120.0 + f * 30.0, 270.0)); // bottom row
            inputs.push(input(20 + i, 110.0, 120.0 + f * 25.0)); // left col
            inputs.push(input(30 + i, 270.0, 120.0 + f * 25.0)); // right col
        }
        let placed = place_labels(rack(), &inputs);
        assert_eq!(placed.len(), inputs.len());
        for p in &placed {
            assert!(
                !p.label_rect.overlaps(rack()),
                "label {} overlaps rack: {:?}",
                p.host_id,
                p.label_rect
            );
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                assert!(
                    !a.label_rect.overlaps(b.label_rect),
                    "labels {} and {} overlap: {:?} vs {:?}",
                    a.host_id,
                    b.host_id,
                    a.label_rect,
                    b.label_rect
                );
            }
        }
    }

    #[test]
    fn oversized_label_owns_its_band_without_overlap() {
        let placed = place_labels(
            rack(),
            &[
                LabelInput {
                    host_id: 0,
                    host_rect: Rect::new(195.0, 110.0, 10.0, 10.0),
                    label_w: 500.0, // wider than the whole rack
                    label_h: 12.0,
                },
                input(1, 195.0, 120.0),
            ],
        );
        assert_eq!(placed.len(), 2);
        let a = &placed[0];
        let b = &placed[1];
        assert!(!a.label_rect.overlaps(rack()));
        assert!(!b.label_rect.overlaps(rack()));
        assert!(!a.label_rect.overlaps(b.label_rect));
    }

    #[test]
    fn anchor_sits_on_host_edge_facing_side() {
        let host = Rect::new(150.0, 150.0, 20.0, 20.0); // center (160,160)
        assert_eq!(edge_anchor(host, Side::Over), Point { x: 160.0, y: 150.0 });
        assert_eq!(edge_anchor(host, Side::Under), Point { x: 160.0, y: 170.0 });
        assert_eq!(edge_anchor(host, Side::Left), Point { x: 150.0, y: 160.0 });
        assert_eq!(edge_anchor(host, Side::Right), Point { x: 170.0, y: 160.0 });
        // Placed anchors agree with the edge helper.
        let placed = place_labels(rack(), &[input(7, 195.0, 110.0)]);
        assert_eq!(placed[0].side, Side::Over);
        assert_eq!(
            placed[0].anchor,
            Point { x: 200.0, y: 110.0 },
            "anchor is the host top edge midpoint"
        );
    }

    #[test]
    fn within_side_ordering_follows_host_coordinate() {
        // Three Over labels out of x-order in the input; bands are wide
        // enough (200px side) that all three share one band in host-x order.
        let placed = place_labels(
            rack(),
            &[
                LabelInput {
                    host_id: 2,
                    host_rect: Rect::new(250.0, 110.0, 10.0, 10.0),
                    label_w: 30.0,
                    label_h: 10.0,
                },
                LabelInput {
                    host_id: 0,
                    host_rect: Rect::new(120.0, 110.0, 10.0, 10.0),
                    label_w: 30.0,
                    label_h: 10.0,
                },
                LabelInput {
                    host_id: 1,
                    host_rect: Rect::new(180.0, 110.0, 10.0, 10.0),
                    label_w: 30.0,
                    label_h: 10.0,
                },
            ],
        );
        assert!(placed.iter().all(|p| p.side == Side::Over));
        let mut by_x = placed.clone();
        by_x.sort_by(|a, b| {
            a.label_rect
                .x
                .partial_cmp(&b.label_rect.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let order: Vec<usize> = by_x.iter().map(|p| p.host_id).collect();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(place_labels(rack(), &[]).is_empty());
    }
}
