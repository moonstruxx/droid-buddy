//! Force-directed layout solver: a one-shot convergence solver, not a
//! simulation (design D1). Bounded iterations of spring attraction along
//! edges + repulsion between nodes + friction damping until total kinetic
//! energy drops below a threshold, then the positions freeze. Re-solve
//! triggers are exactly two: patch load (full `solve`) and user node move
//! (damped `local_resettle`).
//!
//! The seed is a layered (Sugiyama-style) arrangement (design gv5): nodes are
//! placed by longest-path topological depth on x, and within each layer the
//! order on y comes from deterministic barycenter crossing-minimization
//! sweeps instead of file order. Coordinates land exactly on the spacing grid
//! (no jitter), so the convergence target is a clean horizontal chain of
//! aligned columns. The force relaxation then refines from that seed.
//!
//! Deterministic (design D9): every pass — depth, crossing minimization,
//! relaxation — is RNG-free, so the same patch converges to the same
//! arrangement on the same machine. Repulsion uses uniform-grid cell hashing
//! (rebuilt per iteration) so a node only repels against nodes in neighboring
//! cells, keeping the 600-node case near-linear instead of O(n²).
//!
//! Pure module: no terminal dependency. Positions are a `Vec<(f32, f32)>`
//! parallel to `graph.nodes` — index `i` is the position of `graph.nodes[i]`.
//!
//! `solve` is the single convergence entry point; every caller (full graph,
//! re-solves) seeds deterministically from its own graph and converges
//! independently.

use std::collections::{HashMap, HashSet};

use crate::config::{LayoutMode, LayoutOrdering};
use crate::graph::{Graph, NodeId, NodeKind};

/// Freeze when total kinetic energy (sum of |velocity|², unit mass) is below.
const ENERGY_THRESHOLD: f32 = 0.5;
/// Velocity multiplier per iteration in (0, 1); damps motion toward rest.
const FRICTION: f32 = 0.5;
/// Spring rest length between connected nodes.
const SPRING_REST: f32 = 80.0;
/// Spring stiffness along an edge (force = k · (distance − rest)). Tuned so
/// attraction dominates repulsion at patch scale (design D2): connected
/// circuits settle near the rest length while unconnected ones are pushed
/// toward the repulsion radius.
const SPRING_K: f32 = 0.15;

/// Default cable tension — the spring stiffness the solver used before
/// tension became keyboard-adjustable (`[`/`]` on the graph surface). Passing
/// this keeps every existing layout byte-identical (determinism contract:
/// same patch + same machine + same tension → same layout).
pub const DEFAULT_TENSION: f32 = SPRING_K;
/// One keyboard step of cable tension.
pub const TENSION_STEP: f32 = 0.05;
/// Lower clamp for adjustable cable tension.
pub const TENSION_MIN: f32 = 0.05;
/// Upper clamp for adjustable cable tension.
pub const TENSION_MAX: f32 = 0.5;
/// Repulsion magnitude coefficient (force ~ strength / distance²). Softened
/// below spring influence so edges read as the primary structure (design D2).
const REPULSION_STRENGTH: f32 = 1500.0;
/// Repulsion cutoff radius; also the floor of the uniform-grid cell size.
const REPULSION_RADIUS: f32 = 120.0;
/// Default repulsion gain (design D1): the multiplier on the size-aware
/// penetration ramp both arrangements apply. `1.0` reproduces the tuned forces,
/// so [`SolveKnobs::default()`] leaves the sections path byte-identical while
/// the same knob can loosen or stiffen force mode's separation.
pub const REPULSION_GAIN_DEFAULT: f32 = 1.0;
/// Per-axis cap on a single velocity update, keeps the solver from exploding.
const MAX_DISPLACEMENT: f32 = 20.0;

/// Spring stiffness for slice springs in sections mode (design D1 + 3.8):
/// pulls each node toward its own seed target `(slot_x, slot_y + rank · stack
/// pitch)` — see [`sections_seed`]. Mutable at runtime so sliders can tune it
/// (like `tension`).
pub const SLICE_K: f32 = DEFAULT_SLICE_K;
/// Default slice spring stiffness — strong enough to hold non-clustered nodes
/// on their seeded targets while still letting cables drag them within a lane.
pub const DEFAULT_SLICE_K: f32 = 0.1;
/// Spring stiffness for section-hole springs in sections mode (design D1):
/// pulls banner-group members toward their fixed section hole anchor — the mean
/// seed target of their (banner group, row) band ([`section_hole_anchors_lanes`]).
/// Mutable at runtime so sliders can tune it (like `tension`).
pub const SECTION_HOLE_K: f32 = DEFAULT_SECTION_HOLE_K;
/// Default section-hole spring stiffness — anchors members strongly.
pub const DEFAULT_SECTION_HOLE_K: f32 = 0.15;

/// Bounded overlap-removal passes for the **sections and column** paths
/// (sections-black-hole-solver 3.9) and the slack each resolved pair keeps.
/// The force path runs the same sweep but with a converging loop instead of this
/// fixed budget ([`FORCE_SEPARATION_MAX_PASSES`]), because cohesion makes its
/// pass count a function of the knob tuple ([`separate_overlaps_converging`]).
///
/// The wrapped seed is the layout's spacing guarantee — measured worst pairwise
/// penetration 0.00 on `fixtures/own_scale.ini` — but the force relaxation can
/// still destroy it. Every force is clamped to the same [`MAX_DISPLACEMENT`], so
/// a node with an unbalanced cable bundle (own_scale's controller has 16 cables)
/// drifts its entire travel budget — measured 1180 units in both axes, landing
/// inside another row's lane for 79 units of overlap — and a banner band that
/// spans a whole row is compressed toward its own mean the same way. The tether
/// ([`SECTIONS_TETHER`]) stops the runaway; this pass cleans up whatever
/// penetration the remaining slack still produces.
///
/// The relaxation still decides where circuits live (the pass only resolves
/// penetration, never compacts), so the layout keeps its cable-driven character.
const SEPARATION_PASSES: usize = 8;
/// Extra clearance a resolved pair keeps, so a pair the pass touches lands
/// strictly clear instead of exactly touching on the boundary.
const SEPARATION_EPS: f32 = 1.0;

/// Pass cap for the force path's converging separation loop
/// ([`separate_overlaps_converging`]), in whole sweeps.
///
/// The sections and column paths use the fixed [`SEPARATION_PASSES`] budget, but
/// the force relaxation cannot: per-cluster cohesion ([`COHESION_K`]) piles the
/// members of a banner group onto one centroid, so the number of sweeps a layout
/// needs is a property of the knob tuple, not of the arrangement. A sweep that
/// resolves a pair can also push one of its bodies into contact with a neighbour
/// the sweep already visited — which is why the visited-pair count a sweep
/// reports is *not* a residual count — so the loop re-reads the whole pair set
/// before every sweep and stops only when that reading is zero.
///
/// **Measured (2026-10-15, five-fixture set, default tension and repulsion
/// gain)**: at the default cohesion ([`COHESION_K`]) every fixture reaches zero
/// pairs, and the 511-body `fixtures/own_scale.ini` anchor is the worst of them
/// at 18 sweeps (its 17th leaves 1 pair). The cap is 7× that need, and a
/// comfortable cap is close to free: the loop reads the residual *before* its
/// first sweep, so an already-separated layout pays one O(n²) read and no sweep
/// at all.
///
/// The cap is the termination guarantee for a layout the greedy sweep cannot
/// clear in any practical budget — two immovable bodies pinning a pair, or a
/// cohesion pile past its ceiling (at cohesion ≥ 0.0070 that anchor still has
/// 91–274 pairs after 512 sweeps, against 18 sweeps at the default). Beyond the
/// cap the loop returns the residual count instead of iterating, which matters
/// because `solve` runs on the graph surface, where the cost is user-visible.
const FORCE_SEPARATION_MAX_PASSES: usize = 128;

/// Per-axis bound on how far a sections-mode node may be displaced from its own
/// seed target (sections-black-hole-solver 3.9).
///
/// The seed is the spacing contract: it reserves each node's lane, and it is
/// measured overlap-free (worst pairwise penetration 0.00 on
/// `fixtures/own_scale.ini`). The springs then only fine-tune inside that space.
/// Without the tether they do more than fine-tune: cable and section-hole forces
/// are clamped to the same [`MAX_DISPLACEMENT`] budget as the positional springs,
/// so a node with an unbalanced cable bundle drifts its whole travel budget —
/// measured 1180 units in both axes on own_scale — and lands inside another
/// row's lane.
///
/// The value is 1.5 lane gaps, chosen from both ends:
/// - **At least** 43 units: [`sections_mode_keeps_large_nodes_beyond_their_size_required_distance`]
///   needs two capped-lane neighbours (lane pitch 360, required 496) to escape
///   ~43 units each for size-aware repulsion to win. A tighter tether would glue
///   them below their required separation.
/// - **At most** a fraction of the row pitch: two nodes can approach each other
///   by at most `2 × SECTIONS_TETHER`, which the overlap-removal pass then
///   resolves — measured worst residual 1.71 units on own_scale against a 40-unit
///   tolerance, so the slack the tether grants stays far inside the requirement.
///
/// Mutable at runtime? No: unlike [`SLICE_K`] and [`SECTION_HOLE_K`] this is a
/// layout invariant, not a look knob — a larger tether buys nothing but overlap.
pub const SECTIONS_TETHER: f32 = DEFAULT_SECTIONS_TETHER;
/// Default tether: 1.5 lane gaps (see [`SECTIONS_TETHER`]).
pub const DEFAULT_SECTIONS_TETHER: f32 = SECTIONS_LANE_GAP * 1.5;

/// Weak per-cluster cohesion pulling each member toward its banner group's
/// centroid (design D3). Deliberately far below `SPRING_K` so clusters
/// cohere without collapsing or overriding the single-axis spring bias.
///
/// **Measured (2026-10-15, five-fixture set, default tension and repulsion
/// gain)** — this was 0.01 before the force path gained its converging
/// separation loop (task 1.4). The value is bracketed from both sides, and the
/// 511-body `fixtures/own_scale.ini` anchor is the binding fixture:
///
/// - **Ceiling — zero overlapping pairs.** Residual pairs after `solve` on the
///   anchor: 0 up to and including 0.0068 (18 sweeps of the converging pass),
///   137 at 0.0069 with the cap at 128, 86 at 256 and 0 at 384 — just past the
///   ceiling the need jumps ~17× — and 0.0070 and above do not converge at any
///   cap tested up to 512 (91–274 pairs). The other four fixtures stay clear at
///   every value here.
/// - **Floor — banner groups must still read as groups.** On a synthetic graph
///   of two 3-member clusters with no edges the mechanism bifurcates between
///   0.006 (max intra-cluster spread / min inter-cluster gap = 1.98 — clusters
///   interleave) and 0.0065 (0.97 — clusters separate).
///
/// 0.0068 is the largest value inside both bands: 18 sweeps to zero pairs, and
/// 0.94 on that ratio. Its headroom to the ceiling is only 0.0001, so what
/// absorbs a layout whose own ceiling sits lower is the pass cap (7× the
/// measured need), not slack in this constant.
const COHESION_K: f32 = 0.0068;

/// The solver's knob tuple (design D6): one value per force the arrangements
/// use, threaded through every entry point as a bundle instead of as loose
/// arguments.
///
/// The tuple is the layout's determinism key — same patch + same machine +
/// same knob tuple → identical positions — and [`SolveKnobs::default()`]
/// reproduces the tuned defaults, so passing it is exactly today's behaviour
/// (the sections path is byte-identical under it, [`REPULSION_GAIN_DEFAULT`]
/// being 1.0).
#[derive(Clone, Copy, Debug)]
pub struct SolveKnobs {
    /// Cable-spring stiffness, both arrangements (the `[`/`]` knob).
    pub tension: f32,
    /// Sections mode: slice-spring stiffness. Ignored in force mode.
    pub slice_k: f32,
    /// Sections mode: section-hole-spring stiffness. Ignored in force mode.
    pub section_k: f32,
    /// Gain on the size-aware repulsion ramp, both arrangements.
    pub repulsion: f32,
    /// Force mode: per-cluster cohesion strength. Ignored in sections mode
    /// (its section-hole springs do the clustering).
    pub cohesion: f32,
}

impl Default for SolveKnobs {
    fn default() -> Self {
        Self {
            tension: DEFAULT_TENSION,
            slice_k: SLICE_K,
            section_k: SECTION_HOLE_K,
            repulsion: REPULSION_GAIN_DEFAULT,
            cohesion: COHESION_K,
        }
    }
}

/// Horizontal gap between topological-depth levels in the seed layout.
const HORIZONTAL_SPACING: f32 = 80.0;
/// Vertical gap between nodes within one topological layer in the seed layout.
const VERTICAL_SPACING: f32 = 120.0;

/// Number of left→right / right→left barycenter sweeps for within-layer
/// crossing minimization in the layered seed (design gv5). Fixed so the
/// ordering pass is bounded and deterministic.
const CROSSING_SWEEPS: usize = 8;

/// Grid step of the layered seed: coordinates land on multiples of this value
/// (`HORIZONTAL_SPACING` and `VERTICAL_SPACING` are both multiples of it), so
/// the arrangement reads as cleanly aligned columns.
pub const GRID_SNAP: f32 = 10.0;

/// Per-character width of a node title in the size estimator.
const NODE_ESTIMATE_CHAR_WIDTH: f32 = 8.0;
/// Left + right frame padding in the size estimator.
const NODE_ESTIMATE_PADDING: f32 = 16.0;
/// Width reserved per input/output port marker in the size estimator.
const NODE_ESTIMATE_PORT_WIDTH: f32 = 14.0;

/// World-space (solver-unit) body height of a graph node — **not pixels**.
/// The renderer projects it through the graph camera (`pixel = world × zoom`),
/// so a node's drawn frame scales with the zoom instead of staying a fixed
/// pixel size. Deliberately below [`VERTICAL_SPACING`] so nodes stacked in one
/// column keep a visible gap between their bodies at every zoom
/// (graph-zoom-node-scaling, design decision 1).
pub const NODE_WORLD_H: f32 = 80.0;

// Compile-time invariant: the node body must leave a visible gap on a
// vertical slot. Checked at compile time so a constants change fails the build
// instead of tripping `clippy::assertions_on_constants` in a runtime test.
const _: () = assert!(NODE_WORLD_H < VERTICAL_SPACING);

// ── sections-black-hole-solver 3.8: width-aware lanes wrapped into rows ─────

/// Cap on a node's lane-width contribution in sections mode
/// (sections-black-hole-solver 3.8). Lane sizing credits every node
/// `min(estimated_widths[i], SECTIONS_LANE_MAX_NODE_W)`, so one very wide node
/// cannot blow up its lane and shove the wrapped row grid out of square: node
/// widths reach ~496 while [`SECTIONS_LANE_GAP`] is 40, so lane pitch must
/// track width, not a fixed pitch.
///
/// `pub` because the renderer clamps the frame it draws to this same value
/// (task 3.9): a drawn body then never exceeds the lane width the solver
/// reserved for it.
pub const SECTIONS_LANE_MAX_NODE_W: f32 = 240.0;

/// Target row width in world units for the wrapped sections seed
/// (sections-black-hole-solver 3.8). [`sections_lanes`] packs lanes left→right
/// while they fit under this width and wraps to a new row otherwise. Chosen so
/// a ~500-lane patch (e.g. `fixtures/own_scale.ini`, 510 lanes) wraps into a
/// roughly square grid (measured 2732 × 2500, aspect 1.09) instead of the
/// ~40:1 diagonal ribbon the old pitch seed produced.
pub const SECTIONS_ROW_TARGET_W: f32 = 4000.0;

/// Horizontal gap between adjacent lanes inside one row, in world units.
pub const SECTIONS_LANE_GAP: f32 = 40.0;

/// Vertical gap between wrapped rows, in world units.
pub const SECTIONS_ROW_GAP: f32 = 80.0;

/// Vertical gap between nodes stacked inside one lane. `NODE_WORLD_H` + this
/// equals [`VERTICAL_SPACING`] (120) — the in-lane pitch earlier seeds used,
/// so a multi-node lane starts on the same rhythm.
const SECTIONS_STACK_GAP: f32 = 40.0;

/// Width credited to a lane that holds no nodes. An empty section index still
/// consumes its slot so lane order always equals patch order, but it must not
/// reserve a full lane's width.
const SECTIONS_LANE_MIN_W: f32 = 40.0;

/// Default iteration cap for a damped local re-settle (fewer than a solve).
pub const LOCAL_ITERATIONS: usize = 40;
/// Default radius around the moved node that participates in a re-settle.
pub const LOCAL_RADIUS: f32 = 200.0;

/// Iteration budget for a full `solve`. The layered seed is the primary
/// arrangement (design gv5), so the relaxation is a bounded refinement: a
/// modest budget keeps the solve fast and preserves the layered structure
/// while still letting cable tension re-flow the layout. The energy-threshold
/// early exit still fires when the graph settles sooner.
pub const SOLVE_ITERATIONS: usize = 60;

/// Run the full convergence solve from scratch and return frozen positions.
///
/// The returned `Vec<(f32, f32)>` is parallel to `graph.nodes`: index `i`
/// holds the position of `graph.nodes[i]`.
///
/// `pinned` holds node indices that act as unmoved fixed anchors: they stay
/// exactly where the seed placed them while every other node still pulls
/// toward and depends on them. Out-of-range indices are ignored defensively;
/// pass `&[]` for an unpinned solve (today's behavior until task 3.1 supplies
/// real pin indices).
///
/// Used for every solve — each call seeds deterministically from its own
/// graph (topological depth + within-layer order + node-id hash, no RNG) and
/// converges independently under the same bounded, grid-hashed solver
/// (`SOLVE_ITERATIONS`, `ENERGY_THRESHOLD`).
///
/// `knobs` is the solver's knob tuple ([`SolveKnobs`]); its `tension` is the
/// spring stiffness (`SPRING_K`) — pass [`SolveKnobs::default()`] for the tuned
/// defaults. Higher tension pulls cable-connected nodes closer together, lower
/// tension lets repulsion spread them out. Determinism holds per knob tuple:
/// same patch + same machine + same knobs → same layout.
///
/// Beyond the tuned force, the solve runs the **non-overlap post-condition**
/// (design D3): the result passes through the bounded [`separate_overlaps`]
/// pass, which resolves the residual penetration a clamped integrator leaves
/// between drawn bodies. The pass is bounded and pairwise-greedy, not a packing
/// algorithm, so it clears the layouts the solver produces whenever bodies are
/// not piled into the same space — a dense pile can keep a residual.
pub fn solve(graph: &Graph, pinned: &[usize], knobs: SolveKnobs) -> Vec<(f32, f32)> {
    let mut positions = seed_positions(graph);
    run_iterations(
        graph,
        &mut positions,
        SOLVE_ITERATIONS,
        None,
        pinned,
        LayoutMode::Force,
        knobs,
    );
    positions
}

/// Sections-mode solver (sections-black-hole-solver 5.1): joint black-hole solve
/// with slice springs, section-hole springs, cable springs, and size-aware
/// repulsion.
///
/// `knobs` carries the runtime-adjustable stiffnesses (`slice_k` and
/// `section_k`, bound to [`SLICE_K`] and [`SECTION_HOLE_K`] by the caller before
/// calling) plus the cable `tension` and the repulsion gain; pass
/// [`SolveKnobs::default()`] for the tuned defaults.
pub fn solve_sections(graph: &Graph, pinned: &[usize], knobs: SolveKnobs) -> Vec<(f32, f32)> {
    let mut positions = seed_positions_sections(graph);
    run_iterations_with_forces(
        graph,
        &mut positions,
        SOLVE_ITERATIONS,
        None,
        pinned,
        LayoutMode::Sections,
        knobs,
    );
    positions
}

/// Damped local re-settle after a node move (design D1).
///
/// Only nodes within `radius` of the moved node are active and move; distant
/// nodes act as anchors and stay essentially unmoved. `iterations` should be
/// well below `SOLVE_ITERATIONS` (default `LOCAL_ITERATIONS`). `moved` must be a
/// node in `graph`; positions are otherwise left untouched. `pinned` marks
/// node indices that never move — they stay exactly where they currently sit
/// (e.g. a drag-dropped anchor) while still exerting forces on the active
/// neighbourhood. Returns whether the moved node was found. `knobs` is the
/// solver's knob tuple ([`SolveKnobs`]; see [`solve`]).
///
/// The separation post-condition ([`separate_overlaps`]) runs with
/// `movable = active ∧ unpinned` (design D4): a drag can push its own
/// neighbourhood apart, but the anchors outside `radius` never move, so the
/// whole layout can never shift under a drag.
pub fn local_resettle(
    graph: &Graph,
    positions: &mut [(f32, f32)],
    moved: &NodeId,
    radius: f32,
    iterations: usize,
    pinned: &[usize],
    knobs: SolveKnobs,
) -> bool {
    let Some(center) = graph.nodes.iter().position(|n| &n.id == moved) else {
        return false;
    };
    let c = positions[center];
    let active: Vec<bool> = (0..graph.nodes.len())
        .map(|i| dist(positions[i], c) <= radius)
        .collect();
    run_iterations(
        graph,
        positions,
        iterations,
        Some(&active),
        pinned,
        LayoutMode::Force,
        knobs,
    );
    true
}

/// Push every pair of overlapping bodies apart along the axis that needs the
/// least correction, in bounded deterministic passes (sections-black-hole-solver
/// 3.9; the column path shares it under design D3 — see [`SEPARATION_PASSES`]).
/// The force path runs the same sweep through
/// [`separate_overlaps_converging`], which stops on a measured residual instead
/// of a fixed budget.
///
/// This is the **non-overlap post-condition**: it is what makes "no two drawn
/// bodies intersect" hold by construction rather than by tuning, because the
/// integrator is clamped and a strong spring can still drag one body onto
/// another and lose the push.
///
/// `sizes` is `(width, height)` per node (parallel to `positions`), the same
/// extents the renderer draws, so the predicate is the axis-aligned one: a pair
/// is clear once `|Δx| ≥ (w_i + w_j) / 2` *or* `|Δy| ≥ (h_i + h_j) / 2`. Only
/// still-overlapping pairs are touched, and only along their cheaper axis, so
/// the pass never compacts the layout — it strictly removes penetration.
///
/// `movable[i] == false` marks a body the solve may not move at all: a pinned
/// node, or (during a drag) an inactive node outside the moved node's
/// neighbourhood. It stays exactly where it is — it is an anchor other nodes are
/// pushed away from — and its partner takes the whole correction when it can. A
/// pair with both bodies immovable is left alone (see [`solve_columns_pinned`]
/// for the one place that policy yields to the invariant).
///
/// The pass is deliberately *not* tethered: it runs after the relaxation and may
/// push a node past [`SECTIONS_TETHER`] rather than leave two bodies overlapping
/// (measured worst extra push on `fixtures/own_scale.ini`: 13.75 units). It is
/// only needed because the springs can still lose: a lane reserves the width of
/// its own widest member, so a narrow lane butted against a much wider one can
/// seed a pair whose horizontal clearance is short of `(w_a + w_b) / 2` (the one
/// seed-level shortfall the wrapped grid can have).
///
/// Returns the number of pairs still overlapping after the last pass, re-read
/// from the full pair set so the value is a true residual (0 when the layout is
/// clear, which is the common case) rather than the sweep's visited count — see
/// [`separation_sweep`] and [`count_overlapping_pairs`].
///
/// Deterministic: a single thread, a fixed `(i, j)` sweep order, and no RNG —
/// the pass is a pure function of the positions and sizes it is given.
///
/// The force path keeps this exact budget *and* this exact order for its first
/// sweep, then hands over to [`separate_overlaps_converging`], which re-reads the
/// residual after each sweep and cycles through further deterministic orders
/// while pairs persist. Sections and column keep this entry point unchanged, so
/// their output stays byte-identical.
fn separate_overlaps(
    positions: &mut [(f32, f32)],
    sizes: &[(f32, f32)],
    movable: &[bool],
    passes: usize,
) -> usize {
    let order: Vec<usize> = (0..positions.len()).collect();
    for _ in 0..passes {
        if separation_sweep(positions, sizes, movable, &order) == 0 {
            break;
        }
    }
    // The sweep's own count is a work measure (a push can re-introduce a pair it
    // already passed), so the residual is re-read here to give callers an exact
    // "still overlapping" value they can branch on — `solve_columns_pinned` uses
    // it to decide its all-movable second phase.
    count_overlapping_pairs(positions, sizes)
}

/// One separation sweep: walk the pairs in `order` order, resolve each pair whose
/// axis-aligned bodies still intersect along the cheaper axis, and return how
/// many intersecting pairs the sweep *visited*.
///
/// The returned count is a work measure, not a residual: a push can move a body
/// into a pair the sweep already passed. Callers that need the residual use
/// [`count_overlapping_pairs`].
///
/// `movable[i] == false` bodies never move; a pair with both bodies immovable is
/// counted but left alone. Deterministic: pure in the positions, sizes and order
/// it is given.
fn separation_sweep(
    positions: &mut [(f32, f32)],
    sizes: &[(f32, f32)],
    movable: &[bool],
    order: &[usize],
) -> usize {
    let mut overlapping = 0;
    for oi in 0..order.len() {
        let i = order[oi];
        for &j in &order[oi + 1..] {
            let need_x = 0.5 * (sizes[i].0 + sizes[j].0);
            let need_y = 0.5 * (sizes[i].1 + sizes[j].1);
            let dx = positions[j].0 - positions[i].0;
            let dy = positions[j].1 - positions[i].1;
            let pen_x = need_x - dx.abs();
            let pen_y = need_y - dy.abs();
            if pen_x <= 0.0 || pen_y <= 0.0 {
                continue;
            }
            overlapping += 1;
            // How much of the correction each side takes: an immovable
            // partner hands its share to the movable one; two immovable
            // bodies cannot be separated at all.
            let (share_i, share_j) = match (movable[i], movable[j]) {
                (true, true) => (0.5, 0.5),
                (true, false) => (1.0, 0.0),
                (false, true) => (0.0, 1.0),
                (false, false) => (0.0, 0.0),
            };
            // Escape along the cheaper axis (ties go to x, deterministically).
            if pen_x <= pen_y {
                let push = (pen_x + SEPARATION_EPS) * 0.5;
                let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                positions[i].0 -= sign * push * share_i;
                positions[j].0 += sign * push * share_j;
            } else {
                let push = (pen_y + SEPARATION_EPS) * 0.5;
                let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                positions[i].1 -= sign * push * share_i;
                positions[j].1 += sign * push * share_j;
            }
        }
    }
    overlapping
}

/// Pairs whose axis-aligned bodies still intersect — the separation predicate
/// ([`separation_sweep`]) evaluated without moving anything.
///
/// This is the loop condition of [`separate_overlaps_converging`]: the sweep's
/// own visited count cannot tell a cleared layout from one where the sweep
/// pushed a body into a pair it had already passed, so the converging loop's
/// "zero pairs" exit is a fresh reading of the whole pair set, never an
/// inference. Deterministic: O(n²), read-only, no RNG.
fn count_overlapping_pairs(positions: &[(f32, f32)], sizes: &[(f32, f32)]) -> usize {
    let n = positions.len();
    let mut overlapping = 0;
    for i in 0..n {
        for j in i + 1..n {
            let need_x = 0.5 * (sizes[i].0 + sizes[j].0);
            let need_y = 0.5 * (sizes[i].1 + sizes[j].1);
            let dx = (positions[j].0 - positions[i].0).abs();
            let dy = (positions[j].1 - positions[i].1).abs();
            if need_x - dx > 0.0 && need_y - dy > 0.0 {
                overlapping += 1;
            }
        }
    }
    overlapping
}

/// The sweep order of the force path's `pass`-th separation sweep, as an index
/// permutation of `positions`.
///
/// Fixed three-cycle: node index, then bottom-up in y, then left-to-right in x.
/// The cycle is a robustness measure, not a load-bearing trick: measured on the
/// fixture set (2026-10-15) a greedy sweep reaches zero pairs at ≤18 sweeps in
/// *every* ordering, index-only included, and the order only reshapes the
/// intermediate residuals (a pair or two at a given cap, e.g. 23 / 22 / 22 pairs
/// after one sweep on `fixtures/own_scale.ini`). It costs nothing — three sorts
/// per sweep, against an O(n²) sweep — and it keeps a pile from being walked the
/// same way every time when the cheapest-axis escape happens to push against
/// the order.
///
/// A total order (`total_cmp`, node index as the tie-break) keeps the sequence a
/// pure function of the positions: no RNG, no hash iteration.
fn separation_order(positions: &[(f32, f32)], pass: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..positions.len()).collect();
    match pass % 3 {
        1 => order.sort_by(|&a, &b| {
            positions[a]
                .1
                .total_cmp(&positions[b].1)
                .then_with(|| a.cmp(&b))
        }),
        2 => order.sort_by(|&a, &b| {
            positions[a]
                .0
                .total_cmp(&positions[b].0)
                .then_with(|| a.cmp(&b))
        }),
        // pass 0 (and every third pass): node-index order, the same order
        // [`separate_overlaps`] uses, so the first sweep matches it exactly.
        _ => {}
    }
    order
}

/// Force-path separation: sweep until a fresh [`count_overlapping_pairs`] reads
/// zero, cycling the sweep order ([`separation_order`]) while pairs persist and
/// bounded by `max_passes` sweeps.
///
/// This is the force path's half of the non-overlap post-condition (design D3).
/// Unlike [`separate_overlaps`] it makes no promise about *how many* sweeps a
/// layout needs — cohesion can pile a banner group onto a point, and the packet
/// of sweeps a pile needs is a property of the knob tuple. What it does promise
/// is the exit condition: the loop returns only when a full read of the pair set
/// is zero, so a zero return *is* the measured invariant, and a non-zero return
/// is the honest residual of a layout the cap (or an immovable pair) makes
/// unresolvable.
///
/// Deterministic: fixed sweep orders, deterministic tie-breaks, no RNG.
fn separate_overlaps_converging(
    positions: &mut [(f32, f32)],
    sizes: &[(f32, f32)],
    movable: &[bool],
    max_passes: usize,
) -> usize {
    let mut pass = 0;
    loop {
        let remaining = count_overlapping_pairs(positions, sizes);
        if remaining == 0 || pass >= max_passes {
            return remaining;
        }
        let order = separation_order(positions, pass);
        separation_sweep(positions, sizes, movable, &order);
        pass += 1;
    }
}

/// Deterministic initial placement along a single dominant left→right axis
/// (design D1): sources on the left, sinks right. The seed is a layered
/// (Sugiyama-style) arrangement — `x = layer rank · HORIZONTAL_SPACING`,
/// `y = within-layer order · VERTICAL_SPACING` — where the within-layer order
/// comes from deterministic barycenter crossing-minimization sweeps
/// ([`crossing_minimized_orders`]) instead of file order. Coordinates land
/// exactly on the [`GRID_SNAP`] grid (no jitter), so the convergence target is
/// a clean horizontal chain of aligned columns.
///
/// The layer rank is the **dense-normalized** rank of the topological depth
/// ([`dense_columns`]), not the raw depth value, mirroring the column
/// arrangement. The raw depth is a bounded Bellman-Ford relaxation whose
/// *value* is not bounded by the node count on a cycle — each pass adds roughly
/// the cycle length (measured 245,281 on `fixtures/own_scale.ini`, a
/// 19,622,480-unit seed). That seed is unreachable for the integrator, which
/// moves a node at most `MAX_DISPLACEMENT × SOLVE_ITERATIONS` = 1200 units, so
/// the seed decides the layout's order of magnitude: force mode then renders a
/// ~19.6 M-unit smear. Dense-normalizing puts exactly one spacing
/// unit between adjacent *distinct* layers, bounding the seed's horizontal
/// extent by `(distinct layers − 1) · HORIZONTAL_SPACING` — proportional to
/// patch size. It is an order-preserving bijection on distinct depth values, so
/// [`crossing_minimized_orders`] sees identical layers and identical
/// within-layer orders, and a graph whose depth values already form a
/// contiguous `0..N-1` is seeded exactly as before (byte-identical).
pub(crate) fn seed_positions(graph: &Graph) -> Vec<(f32, f32)> {
    // Dense-normalized layered depth (raw depth is unbounded on cycles, see
    // above) with crossing-minimized within-layer order (barycenter sweeps,
    // deterministic). Dense normalization is an order-preserving bijection on
    // distinct depth values, so the crossing-minimization layers are unchanged.
    let depth = dense_columns(&topological_depth(graph));
    let order = crossing_minimized_orders(graph, &depth);

    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let x = depth[i] as f32 * HORIZONTAL_SPACING;
            let y = order[i] as f32 * VERTICAL_SPACING;
            (x, y)
        })
        .collect()
}

/// Wrapped-lane geometry for sections mode (sections-black-hole-solver 3.8).
///
/// A *lane* is one patch-order slice: circuits chain in patch order, controller
/// and input-jack nodes share the fixed left outer lane, output jacks the fixed
/// right outer lane (see [`slice_index`]). Lanes are packed left→right into rows
/// of at most [`SECTIONS_ROW_TARGET_W`], so a 500-lane patch reads roughly
/// square instead of as a long diagonal ribbon.
struct SectionsLanes {
    /// Lane index per node, parallel to `graph.nodes`.
    lane: Vec<usize>,
    /// Slot `(left x, top y)` per lane index, indexable by lane index.
    slot: Vec<(f32, f32)>,
    /// Row index per lane, indexable by lane index. All lanes in one row share
    /// the same `slot.1`; the row index is what lets section-hole anchors be
    /// computed per row band (task 3.9, defect B) instead of from one global
    /// mean slot that a wrapped group cannot reach.
    row: Vec<usize>,
}

impl SectionsLanes {
    /// The slot of the lane the node at `node` belongs to.
    fn slot_of(&self, node: usize) -> (f32, f32) {
        self.slot[self.lane[node]]
    }

    /// The row band the node at `node` wrapped into.
    fn row_of(&self, node: usize) -> usize {
        self.row[self.lane[node]]
    }

    /// Number of wrapped rows (≥ 1 for any non-empty graph).
    fn row_count(&self) -> usize {
        self.row.iter().copied().max().map_or(0, |m| m + 1)
    }
}

/// Sections-mode seed geometry for one graph: the wrapped lanes plus each
/// node's own seed target (task 3.9).
///
/// Both the seed and the slice springs read `targets` from here, so the seed a
/// node starts on and the point its slice spring pulls it back toward can never
/// drift apart (defect A was exactly that divergence: the seed stacked a
/// multi-node lane, the spring flattened it onto one `slot_y`).
struct SectionsSeed {
    lanes: SectionsLanes,
    /// `(slot_x, seed_y)` per node, parallel to `graph.nodes`.
    targets: Vec<(f32, f32)>,
}

/// Assign every node a lane and pack the lanes into wrapped rows
/// (sections-black-hole-solver 3.8).
///
/// Lane index comes from [`slice_index`] (patch order for circuits, fixed outer
/// lanes for controllers/jacks). Lane width is the widest member's
/// `min(estimated_widths[i], SECTIONS_LANE_MAX_NODE_W)` — width *tracking*, so a
/// 240-wide body still fits the lane the solver reserved, while a wider one is
/// capped so it cannot blow the row grid up. Lane height is the lane's **full
/// stacked height** — `(count − 1) · (NODE_WORLD_H + SECTIONS_STACK_GAP) +
/// NODE_WORLD_H` (task 3.9) — so a multi-node lane reserves its whole stack and
/// the next row starts below it. A lane with no nodes keeps its slot with
/// width [`SECTIONS_LANE_MIN_W`], so lane order always equals patch order.
///
/// Packing walks lanes in ascending index order, accumulating `row_x`: a lane
/// starts a new row when placing it would push `row_x + lane_w` past
/// [`SECTIONS_ROW_TARGET_W`] *and* the row already holds a lane. `row_h` is the
/// tallest lane stack in the row, so rows never overlap vertically and each
/// lane's `row` index is recorded for the per-row anchors.
///
/// Deterministic: lane order, lane widths, and the packing walk are pure
/// functions of the graph — no RNG, no hash iteration order.
fn sections_lanes(graph: &Graph) -> SectionsLanes {
    let mut lane = slice_index(graph);

    // Defence in depth against a pathological `section_index` from a corrupt or
    // sparse parse (an absolute index rather than a dense one would otherwise
    // seed an astronomical grid): clamp every lane to the outer-slice bound so
    // a bad input degrades to a wide-but-finite layout instead of a black hole.
    let max_slice = graph
        .nodes
        .iter()
        .map(|node| node.section_index)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for s in lane.iter_mut() {
        *s = (*s).min(max_slice);
    }
    let lane_count = lane.iter().copied().max().map_or(0, |m| m + 1);

    // Lane extents: widest capped member width, and the lane's full stacked
    // height. `lane_w` tracks width; `lane_h` must include the whole stack
    // (task 3.9): a two-node lane reaches `NODE_WORLD_H + SECTIONS_STACK_GAP`
    // below its slot, so a row sized from `NODE_WORLD_H` alone lets that stack
    // overflow into the next row's band (measured on
    // `fixtures/own_scale.ini`: lane 0's second node at y = 120 against row 1's
    // first node at y = 160 — 40 units of vertical overlap in the seed itself).
    let widths = estimated_widths(graph);
    let mut lane_w = vec![0.0f32; lane_count];
    let mut lane_h = vec![0.0f32; lane_count];
    let mut lane_nodes = vec![0usize; lane_count];
    for (i, &s) in lane.iter().enumerate() {
        lane_w[s] = lane_w[s].max(widths[i].min(SECTIONS_LANE_MAX_NODE_W));
        lane_nodes[s] += 1;
    }
    for (s, count) in lane_nodes.iter().enumerate() {
        if *count > 0 {
            lane_h[s] = (*count - 1) as f32 * (NODE_WORLD_H + SECTIONS_STACK_GAP) + NODE_WORLD_H;
        }
    }

    // Pack the lanes into rows, in ascending lane order.
    let mut slot = vec![(0.0f32, 0.0f32); lane_count];
    let mut row = vec![0usize; lane_count];
    let (mut row_x, mut row_y, mut row_h) = (0.0f32, 0.0f32, 0.0f32);
    let mut row_index = 0usize;
    let mut row_has_lane = false;
    for s in 0..lane_count {
        let w = if lane_w[s] > 0.0 {
            lane_w[s]
        } else {
            SECTIONS_LANE_MIN_W
        };
        let h = if lane_h[s] > 0.0 {
            lane_h[s]
        } else {
            SECTIONS_LANE_MIN_W
        };
        if row_has_lane && row_x + w > SECTIONS_ROW_TARGET_W {
            row_y += row_h + SECTIONS_ROW_GAP;
            row_x = 0.0;
            row_h = 0.0;
            row_index += 1;
        }
        slot[s] = (row_x, row_y);
        row[s] = row_index;
        row_x += w + SECTIONS_LANE_GAP;
        row_h = row_h.max(h);
        row_has_lane = true;
    }

    SectionsLanes { lane, slot, row }
}

/// Sections-mode seed: lane geometry plus each node's own stacked target
/// (sections-black-hole-solver 3.9).
///
/// Per node, with `s = lane[i]` and `r` the node's index among its lane's
/// members in graph order:
///
/// ```text
/// x = slot_x(s)                                   (the lane's left edge)
/// y = slot_y(s) + r · (NODE_WORLD_H + SECTIONS_STACK_GAP)
/// ```
///
/// Narrower nodes sit left in their lane; multi-node lanes stack downward at the
/// full [`VERTICAL_SPACING`] pitch. Deterministic: no RNG, no hash iteration
/// order.
fn sections_seed(graph: &Graph) -> SectionsSeed {
    let lanes = sections_lanes(graph);

    // Rank within the lane = number of earlier nodes (graph order) sharing it.
    let mut rank = vec![0usize; graph.nodes.len()];
    let mut next = vec![0usize; lanes.slot.len()];
    for (i, rank_i) in rank.iter_mut().enumerate() {
        let s = lanes.lane[i];
        *rank_i = next[s];
        next[s] += 1;
    }

    let targets = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let (slot_x, slot_y) = lanes.slot_of(i);
            let y = slot_y + rank[i] as f32 * (NODE_WORLD_H + SECTIONS_STACK_GAP);
            (slot_x, y)
        })
        .collect();

    SectionsSeed { lanes, targets }
}

/// Section-hole anchors in lane space, **one per (banner group, row)**
/// (sections-black-hole-solver 3.9, defect B).
///
/// Before 3.9 every member of a group shared the group's single mean lane slot.
/// Once lanes wrap, that mean can sit thousands of units from members that
/// landed in other rows — measured on `fixtures/own_scale.ini`, where each
/// banner group spans 4-5 rows and its members were dragged at the travel cap
/// toward one distant point (worst residual body overlap 79.19).
///
/// Now the anchor a member feels is the mean **seed target**
/// ([`sections_seed`]) of *its own group's circuit members in the same row*: a
/// wrapped group gets one local anchor per row band, so the hole spring
/// compacts each band in place instead of yanking members across the grid. The
/// anchor's y is the mean stacked target y of that band's members, not the
/// row's top edge, so it cannot re-flatten a multi-node lane (defect A's fix).
///
/// The anchor is still a fixed black hole: it derives from the graph's lane
/// geometry only, never from positions, so it is constant for a whole solve.
/// Membership matches [`compute_section_holes`] (whose build-time
/// `Graph.section_holes` anchors keep their own `mean_slice * pitch` meaning):
/// only circuit members of a banner group get `Some`, everything else `None`.
fn section_hole_anchors_lanes(graph: &Graph, seed: &SectionsSeed) -> Vec<Option<(f32, f32)>> {
    let n = graph.nodes.len();
    let mut anchors = vec![None; n];
    if graph.clusters.is_empty() || n == 0 {
        return anchors;
    }

    let rows = seed.lanes.row_count();
    // `(sum_x, sum_y, count)` per (cluster, row) band, indexed flat.
    let mut bands = vec![(0.0f32, 0.0f32, 0usize); graph.clusters.len() * rows];
    for (i, node) in graph.nodes.iter().enumerate() {
        if node.kind != NodeKind::Circuit {
            continue;
        }
        let Some(c) = graph.cluster_index_of(node.section_index) else {
            continue;
        };
        let band = &mut bands[c * rows + seed.lanes.row_of(i)];
        band.0 += seed.targets[i].0;
        band.1 += seed.targets[i].1;
        band.2 += 1;
    }

    for (i, node) in graph.nodes.iter().enumerate() {
        if node.kind != NodeKind::Circuit {
            continue;
        }
        let Some(c) = graph.cluster_index_of(node.section_index) else {
            continue;
        };
        let (sx, sy, count) = bands[c * rows + seed.lanes.row_of(i)];
        debug_assert!(count > 0, "every member counts into its own band");
        anchors[i] = Some((sx / count as f32, sy / count as f32));
    }
    anchors
}

/// Sections-mode seed (sections-black-hole-solver D1 + 3.8): chain the circuits
/// in **patch order** as width-aware lanes, then **wrap the lanes into rows** so
/// a 500-lane patch reads roughly square rather than as a ~40:1 diagonal
/// ribbon.
///
/// The seed *is* the layout: every force is clamped (`≤ MAX_DISPLACEMENT` per
/// axis per iteration, `FRICTION`-damped, `SOLVE_ITERATIONS` total), and since
/// task 3.9 sections mode additionally tethers each node to its own target
/// ([`SECTIONS_TETHER`]), so the forces only fine-tune what the seed places — and
/// the slice spring targets this same per-node target.
///
/// Per node the target is its own seed position `(slot_x, seed_y)` — see
/// [`sections_seed`] for the per-node formula; this is the very same vector the
/// slice springs pull toward, so seed and spring can never disagree. The extent
/// is proportional to patch size (lanes × lane width over the row target), never
/// to cycle length — unlike the raw Bellman-Ford depth the layered seed used to
/// consume, which grows with cycle length on a cyclic patch (measured 245,281
/// on `fixtures/own_scale.ini`). The layered seed now dense-normalizes that
/// depth ([`dense_columns`]) before multiplying by [`HORIZONTAL_SPACING`], so it
/// is bounded by the distinct-layer count in the same way.
///
/// Deterministic: no RNG, no hash iteration order.
pub(crate) fn seed_positions_sections(graph: &Graph) -> Vec<(f32, f32)> {
    sections_seed(graph).targets
}

/// Longest-path topological depth via bounded Bellman-Ford relaxation, shared
/// by the layered seed and the column arrangement. The n-pass bound keeps
/// cyclic graphs finite: each pass relaxes every edge once, so on a cycle
/// depth grows by at most the cycle length per pass and can exceed n — the
/// per-depth maps must not assume depth < n.
fn topological_depth(graph: &Graph) -> Vec<usize> {
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);
    depth_over(graph.nodes.len(), &edges)
}

/// Capped Bellman-Ford longest-path depth over an explicit edge list (node
/// indices into `0..n`). Shared by [`topological_depth`] and the column
/// arrangement's circuit-only depth ([`circuit_depth`]).
fn depth_over(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut depth = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for &(u, v) in edges {
            if depth[v] < depth[u] + 1 {
                depth[v] = depth[u] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    depth
}

/// Map the capped Bellman-Ford depth to dense column indices `0..N-1` (N =
/// distinct depth values), so column indices are contiguous no matter how the
/// depth values are distributed (graph-column-layout D2).
fn dense_columns(depth: &[usize]) -> Vec<usize> {
    let mut distinct: Vec<usize> = depth.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    depth
        .iter()
        .map(|&d| match distinct.binary_search(&d) {
            Ok(i) | Err(i) => i,
        })
        .collect()
}

/// Deterministic column arrangement (graph-column-layout D1, D2 and D3): nodes
/// are assigned to columns by the capped Bellman-Ford topological depth,
/// dense-normalized to `0..N-1`, then placed width-aware. Each column is a
/// block as wide as its widest member; blocks run left to right separated by
/// [`HORIZONTAL_SPACING`]; every node sits centered on its block's center
/// line; nodes stack vertically on distinct [`VERTICAL_SPACING`] slots, so
/// nodes in a column never share vertical space. All coordinates land on the
/// [`GRID_SNAP`] grid. Unlike [`solve`] this path never runs force relaxation.
///
/// Controller and jack nodes sit in fixed outer columns (design D4): the left
/// outer column holds controllers + input jacks, the right outer column holds
/// output jacks, and the circuit columns (circuit-only depth, outer nodes
/// excluded) run between them. Within-column vertical order follows `ordering`
/// (task 1.2): strict slot order by default, barycenter crossing-minimization
/// as the option; column assignment is identical for both.
///
/// `widths` is parallel to `graph.nodes`: entry `i` is node `i`'s width. Pass
/// [`estimated_widths`] to size nodes from the renderer's inputs (title +
/// port markers, task 1.3); missing or negative entries fall back to 0. The
/// returned `Vec<(f32, f32)>` is parallel to `graph.nodes`: index `i` holds
/// node `i`'s position. Pure and deterministic: same graph + same widths +
/// same ordering → identical positions.
pub fn solve_columns(graph: &Graph, widths: &[f32], ordering: LayoutOrdering) -> Vec<(f32, f32)> {
    solve_columns_pinned(graph, widths, ordering, &[])
}

/// Column arrangement with fixed-position anchors (design D6, task 3.2):
/// `pins` holds `(node index, fixed position)` pairs. Each pinned node sits
/// exactly at its fixed position while the remaining nodes arrange around it:
/// unpinned nodes keep the column structure and stack in the slots not
/// occupied by a pinned node in their column. Deterministic per (graph,
/// widths, ordering, pins) — same inputs, same output.
///
/// Pins are honored exactly whenever that is possible; the no-overlap property
/// holds when pinned positions are grid-aligned (as previous column-solve
/// outputs are). The one exception is two pinned bodies that actually
/// intersect: there the invariant wins and only those bodies move — see the
/// separation pass at the end of this function.
pub fn solve_columns_pinned(
    graph: &Graph,
    widths: &[f32],
    ordering: LayoutOrdering,
    pins: &[(usize, (f32, f32))],
) -> Vec<(f32, f32)> {
    let n = graph.nodes.len();
    if n == 0 {
        return Vec::new();
    }
    let width: Vec<f32> = (0..n)
        .map(|i| widths.get(i).copied().unwrap_or(0.0).max(0.0))
        .collect();
    let depth = circuit_depth(graph);
    let (col, ncols) = assign_columns(graph, &depth);
    let rank = column_ranks(graph, &depth, ordering);

    // Block width = widest member, snapped up to 2·GRID_SNAP so the block's
    // center line (every member's x) lands exactly on the grid.
    let block_units = 2.0 * GRID_SNAP;
    let mut block_width = vec![0.0f32; ncols];
    for i in 0..n {
        block_width[col[i]] = block_width[col[i]].max(width[i]);
    }
    for bw in &mut block_width {
        *bw = (*bw / block_units).ceil() * block_units;
    }
    let mut block_left = Vec::with_capacity(ncols);
    let mut left = 0.0f32;
    for &bw in &block_width {
        block_left.push(left);
        left += bw + HORIZONTAL_SPACING;
    }

    // Fixed anchors: pinned nodes keep their given position exactly. Reserve
    // the slot nearest each pinned node's y in its column, so unpinned nodes
    // stack around it without sharing its vertical space.
    let mut fixed: Vec<Option<(f32, f32)>> = vec![None; n];
    let mut reserved: Vec<HashSet<usize>> = vec![HashSet::new(); ncols];
    for &(i, pos) in pins {
        if i >= n {
            continue;
        }
        fixed[i] = Some(pos);
        let slot = (pos.1 / VERTICAL_SPACING).round().max(0.0) as usize;
        reserved[col[i]].insert(slot);
    }

    // Stack slots within each column in rank order (strict = file order,
    // barycenter = crossing-minimized order).
    let mut slot = vec![0usize; ncols];
    let mut positions = vec![(0.0f32, 0.0f32); n];
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| (col[i], rank[i]));
    for i in order {
        if let Some(pos) = fixed[i] {
            positions[i] = pos;
            continue;
        }
        let c = col[i];
        while reserved[c].contains(&slot[c]) {
            slot[c] += 1;
        }
        let x = block_left[c] + block_width[c] / 2.0;
        let y = slot[c] as f32 * VERTICAL_SPACING;
        positions[i] = (x, y);
        slot[c] += 1;
    }

    // Non-overlap is a post-condition of the column path too (design D3/D5).
    // Pins are honored exactly whenever that is possible, but an off-grid pin
    // carried over from a force solve or a drag can land inside a slot
    // neighbour, and two immovable bodies cannot be separated at all — so the
    // policy is two-phase:
    //
    //   1. separate with every pin immovable: in the common case the unpinned
    //      neighbours give way and no pin moves at all;
    //   2. if pairs still intersect, one final pass with every body movable.
    //
    // Exact pin honoring therefore yields to the no-overlap invariant **only**
    // when two pinned bodies intersect — and then only the intersecting bodies
    // actually move, because the first pass already cleared every pair that had
    // a movable partner.
    //
    // The extents are the ones this placement reserved per node (`widths` plus
    // `NODE_WORLD_H`; the app passes `estimated_widths`, which is exactly
    // `node_world_sizes()`' width — ADR 40), so the pass and the placed geometry
    // cannot disagree. `remaining` is a true residual (the pass re-reads the pair
    // set), so the second phase runs only when pairs the pinned-immovable pass
    // could not clear actually remain.
    let sizes: Vec<(f32, f32)> = width.iter().map(|&w| (w, NODE_WORLD_H)).collect();
    let movable: Vec<bool> = (0..n)
        .map(|i| !pins.iter().any(|&(pinned_index, _)| pinned_index == i))
        .collect();
    let remaining = separate_overlaps(&mut positions, &sizes, &movable, SEPARATION_PASSES);
    if remaining > 0 {
        separate_overlaps(&mut positions, &sizes, &vec![true; n], SEPARATION_PASSES);
    }
    positions
}

/// Circuit-only longest-path depth (graph-column-layout D4): the capped
/// Bellman-Ford relaxation over the subgraph induced by circuit nodes, so
/// controller/jack nodes never shift the circuit columns. Non-circuit nodes
/// get depth 0 (their placement is the fixed outer columns, not the depth
/// columns).
fn circuit_depth(graph: &Graph) -> Vec<usize> {
    let n = graph.nodes.len();
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);
    let circuit_edges: Vec<(usize, usize)> = edges
        .iter()
        .copied()
        .filter(|&(u, v)| {
            graph.nodes[u].kind == NodeKind::Circuit && graph.nodes[v].kind == NodeKind::Circuit
        })
        .collect();
    depth_over(n, &circuit_edges)
}

/// Slice index per node for sections mode (sections-black-hole-solver D1):
/// circuit nodes get their `section_index` (patch order 0..N-1), controller and
/// input-jack nodes get the fixed left outer slice (0), and output-jack nodes
/// get the fixed right outer slice (max circuit slice + 1). This mirrors the
/// outer-column assignment in `assign_columns` so slice springs pull circuits
/// to their patch-order columns while controllers/jacks stay at fixed edges.
fn slice_index(graph: &Graph) -> Vec<usize> {
    let n = graph.nodes.len();
    let mut slice = vec![0usize; n];

    // Circuit nodes: slice index = section_index (patch order)
    for (i, node) in graph.nodes.iter().enumerate() {
        if node.kind == NodeKind::Circuit {
            slice[i] = node.section_index;
        }
    }

    // Find max circuit slice for right outer placement
    let max_circuit_slice = graph
        .nodes
        .iter()
        .filter(|nd| nd.kind == NodeKind::Circuit)
        .map(|nd| nd.section_index)
        .max()
        .unwrap_or(0);

    // Controller and InputJack: fixed left outer slice (0)
    // OutputJack: fixed right outer slice (max_circuit_slice + 1)
    for (i, node) in graph.nodes.iter().enumerate() {
        match node.kind {
            NodeKind::Controller | NodeKind::InputJack => {
                slice[i] = 0;
            }
            NodeKind::OutputJack => {
                slice[i] = max_circuit_slice + 1;
            }
            NodeKind::Circuit => {}
        }
    }

    slice
}

/// Build-time section-hole anchor per node (sections-black-hole-solver D2):
/// for each banner group, compute the mean slice of its member circuits (using
/// `slice_index`), then return the anchor at `(mean_slice * HORIZONTAL_SPACING, 0.0)`.
/// Nodes not in any cluster get `None`. The anchor is the same for all members
/// of a cluster (fixed black hole).
///
/// This is the meaning of `Graph.section_holes` recorded at build time and is
/// deliberately unchanged. The *solver* no longer uses it: since task 3.8 the
/// seed and the slice springs work in wrapped-lane slots, so sections mode
/// derives its own fixed anchors in that space via
/// [`section_hole_anchors_lanes`] (mean seed target per group *and row*).
/// Anchoring members
/// at a `mean_slice * HORIZONTAL_SPACING` x would fight the lane grid on any
/// wrapped layout.
///
/// `slice_index` is the output of [`slice_index`] (patch order for circuits,
/// fixed outer slices for controllers/jacks). Deterministic: same graph +
/// same slice_index → identical anchors.
pub fn compute_section_holes(graph: &Graph, slice_index: &[usize]) -> Vec<Option<(f32, f32)>> {
    let n = graph.nodes.len();
    let mut holes = vec![None; n];

    if graph.clusters.is_empty() {
        return holes;
    }

    // member_of[i] = cluster index for node i, or None
    let member_of: Vec<Option<usize>> = (0..n)
        .map(|i| graph.cluster_index_of(graph.nodes[i].section_index))
        .collect();

    // Collect member indices per cluster
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); graph.clusters.len()];
    for (i, cluster) in member_of.iter().enumerate() {
        if let Some(c) = cluster {
            members[*c].push(i);
        }
    }

    // Compute mean slice per cluster (only circuit nodes contribute)
    for list in members.iter() {
        if list.is_empty() {
            continue;
        }
        let mut sum = 0usize;
        let mut count = 0usize;
        for &i in list {
            if graph.nodes[i].kind == NodeKind::Circuit {
                sum += slice_index[i];
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        let mean_slice = sum as f32 / count as f32;
        let anchor = (mean_slice * HORIZONTAL_SPACING, 0.0);
        // Only circuit nodes (banner group members) get the section-hole anchor.
        // Controllers and jacks may have section_index in the cluster range but
        // are not part of the banner group.
        for &i in list {
            if graph.nodes[i].kind == NodeKind::Circuit {
                holes[i] = Some(anchor);
            }
        }
    }

    holes
}

/// Column index per node and total column count (graph-column-layout D4): the
/// left outer column (controllers + input jacks, index 0 when present), the
/// dense-normalized circuit-depth columns, and the right outer column
/// (output jacks, last when present). All-circuit graphs keep the plain
/// dense-normalized assignment.
fn assign_columns(graph: &Graph, depth: &[usize]) -> (Vec<usize>, usize) {
    let n = graph.nodes.len();
    let has_left = graph
        .nodes
        .iter()
        .any(|nd| matches!(nd.kind, NodeKind::Controller | NodeKind::InputJack));
    let has_right = graph.nodes.iter().any(|nd| nd.kind == NodeKind::OutputJack);
    let left_offset = usize::from(has_left);

    // Dense-normalize circuit depths only (outer nodes are excluded from the
    // circuit columns by design D4).
    let circuit_indices: Vec<usize> = (0..n)
        .filter(|&i| graph.nodes[i].kind == NodeKind::Circuit)
        .collect();
    let circ_depths: Vec<usize> = circuit_indices.iter().map(|&i| depth[i]).collect();
    let circ_dense = dense_columns(&circ_depths);
    let n_circ_cols = circ_dense.iter().copied().max().map_or(0, |m| m + 1);

    let mut col = vec![0usize; n];
    for (k, &i) in circuit_indices.iter().enumerate() {
        col[i] = left_offset + circ_dense[k];
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        if node.kind == NodeKind::OutputJack {
            col[i] = left_offset + n_circ_cols;
        }
    }
    let ncols = left_offset + n_circ_cols + usize::from(has_right);
    (col, ncols)
}

/// Within-column vertical order for the column path (task 1.2): strict keeps
/// file order; barycenter re-orders each column by the crossing-minimization
/// sweeps over the circuit subgraph (design gv5 + D4: outer-column nodes are
/// not part of the signal-flow layers and keep file order). Deterministic per
/// (graph, ordering); column assignment is identical for both.
fn column_ranks(graph: &Graph, depth: &[usize], ordering: LayoutOrdering) -> Vec<usize> {
    let n = graph.nodes.len();
    let mut rank: Vec<usize> = (0..n).collect();
    if ordering == LayoutOrdering::Barycenter {
        let index = node_index(graph);
        let edges = edge_pairs(graph, &index);
        let active: Vec<bool> = graph
            .nodes
            .iter()
            .map(|nd| nd.kind == NodeKind::Circuit)
            .collect();
        let layers = crossing_minimized_layers(n, &edges, depth, &active);
        for layer in &layers {
            for (pos, &i) in layer.iter().enumerate() {
                rank[i] = pos;
            }
        }
    }
    rank
}

/// Title mirroring the renderer's `instance_title` (src/gui/graph.rs): the
/// first occurrence of a single-instance circuit is the plain name; among
/// repeated instances the first stays plain and later occurrences carry the
/// zero-based index (`copy`, `copy (1)`, ...).
fn estimated_title(name: &str, index: usize, repeats: Option<usize>) -> String {
    match repeats {
        Some(n) if n > 1 && index > 0 => format!("{name} ({index})"),
        _ => name.to_string(),
    }
}

/// Estimated width of every node (graph-column-layout D3, task 1.3): the
/// renderer-input sizes that feed the column placement, so wide nodes widen
/// their column. Each width is title characters at
/// [`NODE_ESTIMATE_CHAR_WIDTH`] each, frame padding ([`NODE_ESTIMATE_PADDING`]),
/// plus a fixed marker width per input/output port the node has edges for
/// ([`NODE_ESTIMATE_PORT_WIDTH`]) — the same inputs the renderer uses (title
/// from `instance_title`, port presence from incidence). Non-circuit nodes
/// (controllers/jacks) have no instance suffix, mirroring the renderer.
/// Deterministic; rendering remains size-authoritative.
///
/// O(n + e): the node index, edge incidence, and per-circuit instance counts
/// are each computed once for the whole graph. The renderer calls this on
/// every frame (scene build plus pointer hit-testing), so the previous
/// per-node recomputation — which rebuilt the node index and scanned every
/// node and edge for each node, O(n·(n+e)) — froze the window on large
/// patches. Output is unchanged.
pub fn estimated_widths(graph: &Graph) -> Vec<f32> {
    let n = graph.nodes.len();
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);
    let mut has_in = vec![false; n];
    let mut has_out = vec![false; n];
    for &(u, v) in &edges {
        has_out[u] = true;
        has_in[v] = true;
    }
    let mut repeats: HashMap<&str, usize> = HashMap::new();
    for node in &graph.nodes {
        if node.kind == NodeKind::Circuit {
            *repeats.entry(node.circuit.as_str()).or_default() += 1;
        }
    }
    (0..n)
        .map(|i| {
            let node = &graph.nodes[i];
            let title = if node.kind == NodeKind::Circuit {
                let count = repeats.get(node.circuit.as_str()).copied().unwrap_or(0);
                estimated_title(&node.circuit, node.instance_index, Some(count))
            } else {
                node.circuit.clone()
            };
            let title_w = title.chars().count() as f32 * NODE_ESTIMATE_CHAR_WIDTH;
            let ports_w = NODE_ESTIMATE_PORT_WIDTH
                * (usize::from(has_in[i]) + usize::from(has_out[i])) as f32;
            title_w + NODE_ESTIMATE_PADDING + ports_w
        })
        .collect()
}

/// World-space body size `(width, height)` of every node
/// (graph-zoom-node-scaling, design decision 1): the width is this node's
/// [`estimated_widths`] entry — the same estimate the column arrangement
/// reserves per node, so a drawn frame always fits the space the solver gave
/// it — and the height is the fixed [`NODE_WORLD_H`]. The renderer multiplies
/// these by the camera zoom, which is what makes node bodies scale with zoom
/// and keeps the column arrangement overlap-free at any zoom. Parallel to
/// `graph.nodes`; pure and deterministic.
pub fn node_world_sizes(graph: &Graph) -> Vec<(f32, f32)> {
    estimated_widths(graph)
        .into_iter()
        .map(|w| (w, NODE_WORLD_H))
        .collect()
}

/// Core barycenter sweeps (design gv5): group the `active` node indices into
/// layers by `depth`, run [`CROSSING_SWEEPS`] alternating-direction passes
/// that re-order each layer by the mean (barycenter) of its neighbors' current
/// orders in the adjacent layer (stable sort, so equal barycenters keep their
/// current relative order), and return the re-ordered layers. `active` lets
/// callers exclude nodes that must not join the sweeps (outer-column nodes,
/// design D4) without building a subgraph. No RNG and a fixed sweep count
/// make the result deterministic: same inputs → same layers.
fn crossing_minimized_layers(
    n: usize,
    edges: &[(usize, usize)],
    depth: &[usize],
    active: &[bool],
) -> Vec<Vec<usize>> {
    // Group node indices by depth into layers, preserving file order within
    // each layer. Depth can exceed `n` on cyclic graphs, so layers live in a
    // map keyed by depth, not a fixed-size vec.
    let mut layers: Vec<Vec<usize>> = Vec::new();
    let mut layer_of_depth: HashMap<usize, usize> = HashMap::new();
    for (i, &d) in depth.iter().enumerate().take(n) {
        if !active[i] {
            continue;
        }
        let li = *layer_of_depth.entry(d).or_insert_with(|| {
            layers.push(Vec::new());
            layers.len() - 1
        });
        layers[li].push(i);
    }

    // Current within-layer order per node (0-based).
    let mut order = vec![0usize; n];
    for layer in &layers {
        for (pos, &i) in layer.iter().enumerate() {
            order[i] = pos;
        }
    }

    for sweep in 0..CROSSING_SWEEPS {
        let left_to_right = sweep % 2 == 0;
        let layer_indices: Vec<usize> = if left_to_right {
            (0..layers.len()).collect()
        } else {
            (0..layers.len()).rev().collect()
        };
        for &li in &layer_indices {
            let adj_li = if left_to_right {
                if li == 0 {
                    continue;
                }
                li - 1
            } else {
                if li + 1 >= layers.len() {
                    continue;
                }
                li + 1
            };
            let layer = &layers[li];
            let adj_layer = &layers[adj_li];

            // Membership masks for O(1) edge classification.
            let mut in_layer = vec![false; n];
            for &i in layer {
                in_layer[i] = true;
            }
            let mut in_adj = vec![false; n];
            for &i in adj_layer {
                in_adj[i] = true;
            }

            // Barycenter = mean of neighbor orders in the adjacent layer.
            let mut sum = vec![0.0f32; n];
            let mut count = vec![0usize; n];
            for &(u, v) in edges {
                if in_layer[u] && in_adj[v] {
                    sum[u] += order[v] as f32;
                    count[u] += 1;
                } else if in_layer[v] && in_adj[u] {
                    sum[v] += order[u] as f32;
                    count[v] += 1;
                }
            }

            // Stable sort by barycenter; neighbor-less nodes keep their
            // current order (barycenter = current order). `sort_by` is stable,
            // so equal barycenters preserve the current relative order.
            let mut sorted = layer.clone();
            sorted.sort_by(|&a, &b| {
                let ba = if count[a] > 0 {
                    sum[a] / count[a] as f32
                } else {
                    order[a] as f32
                };
                let bb = if count[b] > 0 {
                    sum[b] / count[b] as f32
                } else {
                    order[b] as f32
                };
                ba.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
            });

            for (pos, &i) in sorted.iter().enumerate() {
                order[i] = pos;
            }
            layers[li] = sorted;
        }
    }

    layers
}

/// Deterministic within-layer ordering that reduces edge crossings between
/// adjacent layers (barycenter heuristic, design gv5).
///
/// Wrapper over [`crossing_minimized_layers`] flattening the re-ordered
/// layers into a per-node within-layer order for the layered seed
/// ([`seed_positions`]). No RNG and a fixed sweep count make the result
/// deterministic: same graph → same order.
fn crossing_minimized_orders(graph: &Graph, depth: &[usize]) -> Vec<usize> {
    let n = graph.nodes.len();
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);
    let layers = crossing_minimized_layers(n, &edges, depth, &vec![true; n]);
    let mut order = vec![0usize; n];
    for layer in &layers {
        for (pos, &i) in layer.iter().enumerate() {
            order[i] = pos;
        }
    }
    order
}

/// Advance `positions` up to `max_iter` iterations, returning how many were
/// actually run so callers can detect early energy-threshold convergence.
/// `active` (when `Some`) marks which nodes may move; inactive nodes stay put
/// but still exert forces (they anchor the active set). `pinned` marks node
/// indices that never move, exactly like inactive nodes but for the whole
/// solve (fixed anchors; out-of-range indices are ignored). Freezes early
/// when kinetic energy converges.
///
/// `mode` selects the force model:
/// - `LayoutMode::Force`: cable springs + size-aware repulsion + weak cohesion
/// - `LayoutMode::Sections`: slice springs, section-hole springs, cable springs,
///   and size-aware repulsion (cohesion replaced by hole springs), each node
///   tethered to its own seed target ([`SECTIONS_TETHER`])
///
/// `knobs` is the solver's knob tuple ([`SolveKnobs`]), the determinism key and
/// the only source of stiffness and gain values.
///
/// Both models end with the same **non-overlap post-condition** (design D3):
/// the bounded [`separate_overlaps`] pass, so the invariant comes from an
/// explicit pass rather than from tuning the forces. Pinned nodes, and (for a
/// drag) inactive nodes, are immovable in that pass, so only bodies free to
/// move are ever pushed.
pub(crate) fn run_iterations(
    graph: &Graph,
    positions: &mut [(f32, f32)],
    max_iter: usize,
    active: Option<&[bool]>,
    pinned: &[usize],
    mode: LayoutMode,
    knobs: SolveKnobs,
) -> usize {
    run_iterations_with_forces(graph, positions, max_iter, active, pinned, mode, knobs)
}

/// Core solver behind [`run_iterations`]: one force model per arrangement, both
/// driven entirely by [`SolveKnobs`].
///
/// `knobs.tension` is the cable-spring stiffness (both arrangements);
/// `knobs.slice_k` and `knobs.section_k` are the sections-mode slice and
/// section-hole stiffnesses (ignored in [`LayoutMode::Force`]);
/// `knobs.repulsion` scales the size-aware penetration ramp (both arrangements);
/// `knobs.cohesion` is the force-mode per-cluster cohesion (unused in sections
/// mode).
pub(crate) fn run_iterations_with_forces(
    graph: &Graph,
    positions: &mut [(f32, f32)],
    max_iter: usize,
    active: Option<&[bool]>,
    pinned: &[usize],
    mode: LayoutMode,
    knobs: SolveKnobs,
) -> usize {
    let n = graph.nodes.len();
    if n == 0 {
        return 0;
    }
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);
    let pinned_mask: Vec<bool> = (0..n).map(|i| pinned.contains(&i)).collect();
    // A cable bundle between the same two circuits is one connection: count
    // parallel edges so the spring force is applied once per pair, not once
    // per cable (design gv5). Without this, a button feeding 11 inputs of one
    // circuit creates an 11×-stiff oscillator that never settles.
    let mut edge_multiplicity: HashMap<(usize, usize), usize> = HashMap::new();
    for &(u, v) in &edges {
        *edge_multiplicity.entry((u, v)).or_insert(0) += 1;
    }
    // Cluster membership for cohesion / section holes; `None` = node in no banner
    // group.
    let member_of: Vec<Option<usize>> = (0..n)
        .map(|i| graph.cluster_index_of(graph.nodes[i].section_index))
        .collect();

    // Body extents as the renderer draws them (ADR 40): `(width, height)` per
    // node. `sizes` drives the axis-aligned overlap-removal pass; `extents` (the
    // `max(width, height)` each body claims around its center) drives the radial
    // size-aware repulsion. Both arrangements read the same extents, so the force
    // that separates a pair and the pass that guarantees it always agree with the
    // frame the user sees.
    let sizes = node_world_sizes(graph);
    let extents: Vec<f32> = sizes.iter().map(|&(w, h)| w.max(h)).collect();
    // Grid cell size for this graph (design D2): never smaller than
    // `REPULSION_RADIUS` (keeps the 3x3 neighbour scan valid for the small-node
    // case) and never smaller than the largest node extent. A pair only repels
    // while its center distance is below `required = 0.5 * (extent_i +
    // extent_j) <= max_extent`, and any two points within one cell size differ by
    // at most one cell per axis, so the 3x3 scan cannot miss a repelling pair —
    // in either arrangement. The falloff keeps its own `REPULSION_RADIUS` cutoff,
    // so widening the cell does not extend the long-range force's reach.
    let cell_size = REPULSION_RADIUS.max(extents.iter().copied().fold(0.0f32, f32::max));

    // Sections-mode precomputation: wrapped lane slots (the targets of the
    // slice springs) and section-hole anchors in lane-slot space (computed once
    // — fixed black holes). Force mode has neither.
    let (node_targets, holes) = if mode == LayoutMode::Sections {
        let seed = sections_seed(graph);
        let holes = section_hole_anchors_lanes(graph, &seed);
        (Some(seed.targets), Some(holes))
    } else {
        (None, None)
    };

    let mut velocity = vec![(0.0f32, 0.0f32); n];

    let mut iterations = 0;
    for _ in 0..max_iter {
        iterations += 1;
        // Rebuild the uniform repulsion grid from current positions (D9). The
        // cell is graph-adaptive in both arrangements (see `cell_size` above) so
        // the 3x3 neighbour scan covers every pair the size-aware force reaches.
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, &pos) in positions.iter().enumerate().take(n) {
            grid.entry(cell_of(pos, cell_size)).or_default().push(i);
        }

        let mut accel = vec![(0.0f32, 0.0f32); n];

        // Cable springs (both modes): bundle-normalized spring force along edges.
        for &(u, v) in &edges {
            let m = edge_multiplicity[&(u, v)] as f32;
            let f = spring_force(positions[u], positions[v], knobs.tension);
            accel[u].0 += f.0 / m;
            accel[u].1 += f.1 / m;
            accel[v].0 -= f.0 / m;
            accel[v].1 -= f.1 / m;
        }

        // Repulsion and mode-specific springs.
        if mode == LayoutMode::Sections {
            // Sections mode: slice springs + section-hole springs + size-aware repulsion
            let node_targets = node_targets.as_ref().unwrap();
            let hole_anchors = holes.as_ref().unwrap();

            // Slice springs: pull each node toward its own seed target in both
            // axes — `(slot_x, slot_y + rank · stack pitch)`, so a multi-node
            // lane's stacked members keep the full pitch instead of collapsing
            // onto one `slot_y` (task 3.9, defect A). The seed already sits on
            // that target, and this is what keeps the wrapped row structure
            // stable under cable pull. Deliberately weak (SLICE_K): cables can
            // still drag a node within its lane band, and repulsion can spread
            // the members of a multi-node lane further.
            for (i, &pos) in positions.iter().enumerate().take(n) {
                let (target_x, target_y) = node_targets[i];
                accel[i].0 += (target_x - pos.0) * knobs.slice_k;
                accel[i].1 += (target_y - pos.1) * knobs.slice_k;
            }

            // Section-hole springs: pull cluster members toward their fixed anchor
            // (both x and y). Anchors are fixed black holes computed once at solve start.
            // Only circuit nodes feel the section-hole spring; controllers/jacks only feel
            // slice springs and repulsion (D2: sections are circuit groupings).
            for (i, &pos) in positions.iter().enumerate().take(n) {
                if let Some((anchor_x, anchor_y)) = hole_anchors[i] {
                    if graph.nodes[i].kind == NodeKind::Circuit {
                        accel[i].0 += (anchor_x - pos.0) * knobs.section_k;
                        accel[i].1 += (anchor_y - pos.1) * knobs.section_k;
                    }
                }
            }

            // Size-aware repulsion (design D1): two bodies repel until their
            // center distance reaches `required = 0.5 * (extent_i + extent_j)`,
            // the separation at which their `max(width, height)` extents stop
            // overlapping. `cell_size` was sized from the largest extent, so the
            // 3x3 scan below sees every such pair.
            for (i, &pos) in positions.iter().enumerate().take(n) {
                let (cx, cy) = cell_of(pos, cell_size);
                let extent_i = extents[i];
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let Some(neighbors) = grid.get(&(cx + dx, cy + dy)) else {
                            continue;
                        };
                        for &j in neighbors {
                            if i >= j {
                                continue;
                            }
                            let required = 0.5 * (extent_i + extents[j]);
                            let f = repulsion_force_size_aware(
                                pos,
                                positions[j],
                                required,
                                knobs.repulsion,
                            );
                            accel[i].0 += f.0;
                            accel[i].1 += f.1;
                            accel[j].0 -= f.0;
                            accel[j].1 -= f.1;
                        }
                    }
                }
            }
            // Note: cohesion is intentionally omitted in sections mode —
            // section-hole springs provide the clustering.
        } else {
            // Force mode: size-aware repulsion + weak cohesion.
            //
            // The predicate is the same one sections mode uses — the linear
            // penetration ramp inside `required` (design D1/D3), because a point
            // force with a fixed cutoff cannot separate wide bodies: node
            // extents reach ~496 while `REPULSION_RADIUS` is 120, so two wide
            // bodies were never pushed apart at all. Beyond `required` the
            // solver keeps the historical inverse-square falloff, so the spread
            // between *already clear* neighbours is unchanged and long chains
            // cannot collapse; that falloff keeps its own `REPULSION_RADIUS`
            // cutoff, so the wider grid cell only lets the ramp see wide pairs.
            for (i, &pos) in positions.iter().enumerate().take(n) {
                let (cx, cy) = cell_of(pos, cell_size);
                let extent_i = extents[i];
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let Some(neighbors) = grid.get(&(cx + dx, cy + dy)) else {
                            continue;
                        };
                        for &j in neighbors {
                            if i >= j {
                                continue;
                            }
                            let required = 0.5 * (extent_i + extents[j]);
                            let f = repulsion_force_with_falloff(
                                pos,
                                positions[j],
                                required,
                                knobs.repulsion,
                            );
                            accel[i].0 += f.0;
                            accel[i].1 += f.1;
                            accel[j].0 -= f.0;
                            accel[j].1 -= f.1;
                        }
                    }
                }
            }

            // Per-cluster cohesion: pull members toward their centroid (D3).
            if !graph.clusters.is_empty() {
                let mut members: Vec<Vec<usize>> = vec![Vec::new(); graph.clusters.len()];
                for (i, cluster) in member_of.iter().enumerate() {
                    if let Some(c) = cluster {
                        members[*c].push(i);
                    }
                }
                for list in &members {
                    if list.is_empty() {
                        continue;
                    }
                    let (mut sx, mut sy) = (0.0f32, 0.0f32);
                    for &i in list {
                        sx += positions[i].0;
                        sy += positions[i].1;
                    }
                    let (cx, cy) = (sx / list.len() as f32, sy / list.len() as f32);
                    for &i in list {
                        accel[i].0 += (cx - positions[i].0) * knobs.cohesion;
                        accel[i].1 += (cy - positions[i].1) * knobs.cohesion;
                    }
                }
            }
        }

        let mut kinetic = 0.0f32;
        for i in 0..n {
            if pinned_mask[i] || active.is_some_and(|act| !act[i]) {
                continue;
            }
            let a = clamp(accel[i], MAX_DISPLACEMENT);
            velocity[i].0 = (velocity[i].0 + a.0) * FRICTION;
            velocity[i].1 = (velocity[i].1 + a.1) * FRICTION;
            positions[i].0 += velocity[i].0;
            positions[i].1 += velocity[i].1;
            kinetic += velocity[i].0 * velocity[i].0 + velocity[i].1 * velocity[i].1;
        }

        // Sections mode: a node may only fine-tune inside the space the seed
        // reserved for it (see [`SECTIONS_TETHER`]). Applied to the position
        // rather than to the acceleration so it bounds the whole trajectory: a
        // runaway node can no longer drag its cable neighbours across the grid.
        if mode == LayoutMode::Sections {
            let node_targets = node_targets.as_ref().unwrap();
            for i in 0..n {
                if pinned_mask[i] || active.is_some_and(|act| !act[i]) {
                    continue;
                }
                let (target_x, target_y) = node_targets[i];
                positions[i].0 = target_x + clamp_axis(positions[i].0 - target_x, SECTIONS_TETHER);
                positions[i].1 = target_y + clamp_axis(positions[i].1 - target_y, SECTIONS_TETHER);
            }
        }

        if kinetic < ENERGY_THRESHOLD {
            break;
        }
    }

    // Non-overlap is a post-condition of every path that produces positions
    // (design D3), not a tuning hope: the integrator is clamped, so a strong
    // spring can still drag one body onto another and lose the push. The pass is
    // bounded and deterministic and never compacts — it only resolves
    // penetration, so the relaxation still decides where circuits live.
    //
    // `movable` mirrors the integrator's own policy, so separation can never
    // move a body the solve was not allowed to move: a pinned node stays exactly
    // where it is (an anchor others are pushed away from), and an inactive node
    // — outside a drag's neighbourhood, or an anchor in a local re-settle —
    // stays exactly where it is too (design D4). For `solve` and `local_resettle`
    // that is the whole policy: exact pin honoring yields to the invariant only
    // on the column path (see [`solve_columns_pinned`]), where pins carry
    // off-grid positions from a previous force solve or drag.
    //
    // Sections keeps the fixed `SEPARATION_PASSES` budget *and* the single
    // node-index sweep order, so its output is byte-identical under the knob
    // plumbing. Force mode converges instead ([`separate_overlaps_converging`]):
    // cohesion piles a banner group onto one centroid, and how many sweeps that
    // pile needs is a property of the knob tuple, not of the arrangement.
    let movable: Vec<bool> = (0..n)
        .map(|i| !pinned_mask[i] && !active.is_some_and(|act| !act[i]))
        .collect();
    if mode == LayoutMode::Sections {
        separate_overlaps(positions, &sizes, &movable, SEPARATION_PASSES);
    } else {
        separate_overlaps_converging(positions, &sizes, &movable, FORCE_SEPARATION_MAX_PASSES);
    }
    iterations
}

/// Force on `a` pulling it toward `b` along the edge (Hooke's law).
/// `tension` is the spring stiffness; higher values pull harder per unit
/// stretch, so connected nodes settle closer together.
fn spring_force(a: (f32, f32), b: (f32, f32), tension: f32) -> (f32, f32) {
    let (dx, dy, d) = delta(a, b);
    if d < 1e-6 {
        return (0.0, 0.0);
    }
    let mag = tension * (d - SPRING_REST);
    ((dx / d) * mag, (dy / d) * mag)
}

/// Force on `a` pushing it away from `b` (inverse-square, radius-capped).
///
/// Since design D1 this is the **long-range term** of the force-mode repulsion
/// rather than that arrangement's whole force: it is what spaces pairs whose
/// bodies are already clear of each other, and its own [`REPULSION_RADIUS`]
/// cutoff is deliberately unchanged, so widening the repulsion grid cell lets
/// the penetration ramp see wide pairs without extending this term's reach.
fn repulsion_force(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy, d) = delta(a, b);
    if d < 1e-6 {
        // Coincident nodes: a small deterministic nudge breaks the tie.
        return (0.001, 0.0);
    }
    if d > REPULSION_RADIUS {
        return (0.0, 0.0);
    }
    let mag = (REPULSION_STRENGTH / (d * d)).min(MAX_DISPLACEMENT);
    ((-dx / d) * mag, (-dy / d) * mag)
}

/// Magnitude of the size-aware penetration ramp both arrangements share
/// (design D1/D3): linear in the penetration `required - d`, saturating at the
/// per-axis step budget [`MAX_DISPLACEMENT`] after one [`REPULSION_RADIUS`] of
/// penetration. Normalizing by that constant radius (not by `required` or
/// `required²`) keeps both regimes in budget, and it is the one formula behind
/// the force and the sections path, so both arrangements speak one separation
/// language.
fn penetration_ramp_mag(d: f32, required: f32) -> f32 {
    (MAX_DISPLACEMENT * (required - d) / REPULSION_RADIUS).min(MAX_DISPLACEMENT)
}

/// Size-aware repulsion for the sections arrangement (design D3): push `a` away
/// from `b` until their center distance reaches `required = 0.5 * (extent_a +
/// extent_b)` — the separation at which their `max(width, height)` bounding
/// extents stop overlapping. `gain` scales the ramp; `1.0`
/// ([`REPULSION_GAIN_DEFAULT`]) is byte-identical to the pre-gain solver.
///
/// Properties this shape is chosen for:
/// - **Zero at `d >= required`** — no phantom push at rest; the pair stops as
///   soon as its bodies clear.
/// - **Monotone and finite as `d -> 0`** — the ramp only grows with penetration,
///   and the coincident case returns a fixed nudge first, so there is no
///   division by zero.
/// - **Scale-free in `required`** — the ramp length is the constant
///   [`REPULSION_RADIUS`], not `required`. A wide pair (large `required`) is
///   therefore pushed hard for most of its travel and still held firmly near its
///   required separation, instead of decaying into nothing at the boundary. That
///   is the reason for this form over the old inverse-square
///   `REPULSION_STRENGTH / d²` falloff: at the required distance that falloff has
///   decayed to `REPULSION_STRENGTH / required²` (≈ 0.006 for a 496-wide pair),
///   far too weak to expand the pair within [`SOLVE_ITERATIONS`] — which is why
///   wide bodies never reached their required separation. Normalizing by the
///   constant radius (rather than by `required` or `required²`) keeps both
///   regimes in budget: the slope stays below the step budget for a small pair
///   (`required` ≈ 80), so cluster members are nudged rather than flung, while a
///   wide pair still saturates the budget for as long as it has to travel.
fn repulsion_force_size_aware(
    a: (f32, f32),
    b: (f32, f32),
    required: f32,
    gain: f32,
) -> (f32, f32) {
    let (dx, dy, d) = delta(a, b);
    if d < 1e-6 {
        // Coincident nodes: a small deterministic nudge breaks the tie.
        return (0.001, 0.0);
    }
    if required <= 0.0 || d >= required {
        // At or beyond the required separation the pair is clear: no force.
        return (0.0, 0.0);
    }
    let mag = penetration_ramp_mag(d, required) * gain;
    ((-dx / d) * mag, (-dy / d) * mag)
}

/// Force-mode repulsion (design D1): the size-aware penetration ramp *inside*
/// `required`, then the retained inverse-square falloff beyond it, both scaled by
/// `gain`.
///
/// ```text
/// required = 0.5 * (extent_i + extent_j)     // extent = max(width, height)
/// d <  required: mag = MAX_DISPLACEMENT * min(1, (required - d) / REPULSION_RADIUS)
/// d >= required: mag = min(MAX_DISPLACEMENT, REPULSION_STRENGTH / d²)
///                      (zero beyond REPULSION_RADIUS)
/// mag *= gain
/// ```
///
/// Keeping the falloff is what preserves the arrangement's character: it is the
/// term that spaces neighbours that are already clear, so removing it would
/// change the whole layout rather than add the missing separation. The ramp
/// engages only where two `node_world_sizes()` bodies actually intersect (ADR
/// 40) and is bounded by [`MAX_DISPLACEMENT`], so it can only push an overlapping
/// pair apart — never blow the layout up. Where `d < required`, the ramp
/// *replaces* the falloff (which at those distances is both weak and, for wide
/// pairs, below the cutoff entirely).
fn repulsion_force_with_falloff(
    a: (f32, f32),
    b: (f32, f32),
    required: f32,
    gain: f32,
) -> (f32, f32) {
    let (dx, dy, d) = delta(a, b);
    if d < 1e-6 {
        // Coincident nodes: the same deterministic nudge the other forms use.
        return (0.001, 0.0);
    }
    let mag = if required > 0.0 && d < required {
        penetration_ramp_mag(d, required) * gain
    } else {
        // Bodies clear of each other: the historical falloff decides, gain-scaled
        // (and zero beyond its own REPULSION_RADIUS cutoff).
        let (fx, fy) = repulsion_force(a, b);
        return (fx * gain, fy * gain);
    };
    ((-dx / d) * mag, (-dy / d) * mag)
}

fn delta(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let d = (dx * dx + dy * dy).sqrt();
    (dx, dy, d)
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    delta(a, b).2
}

fn clamp(v: (f32, f32), max: f32) -> (f32, f32) {
    (v.0.clamp(-max, max), v.1.clamp(-max, max))
}

/// Clamp a single offset to `±max` (the per-axis form of [`clamp`], used by the
/// sections-mode tether).
fn clamp_axis(v: f32, max: f32) -> f32 {
    v.clamp(-max, max)
}

fn cell_of(p: (f32, f32), cell_size: f32) -> (i32, i32) {
    (
        (p.0 / cell_size).floor() as i32,
        (p.1 / cell_size).floor() as i32,
    )
}

fn node_index(graph: &Graph) -> HashMap<&NodeId, usize> {
    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (&node.id, i))
        .collect()
}

/// Resolve edges to node indices; edges referencing a missing node are skipped.
fn edge_pairs(graph: &Graph, index: &HashMap<&NodeId, usize>) -> Vec<(usize, usize)> {
    graph
        .edges
        .iter()
        .filter_map(|e| {
            let source = *index.get(&e.source)?;
            let sink = *index.get(&e.sink)?;
            Some((source, sink))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Cluster, GraphEdge, GraphNode, GraphOptions, NodeId, NodeKind};
    use crate::latency::CostModel;
    use crate::patch::Patch;
    use std::path::Path;

    fn node(name: &str, section_index: usize) -> GraphNode {
        GraphNode {
            id: NodeId::circuit(name, 0),
            kind: NodeKind::Circuit,
            circuit: name.to_string(),
            instance_index: 0,
            section_index,
        }
    }

    /// Circuit node with an explicit occurrence index (for repeated instances,
    /// mirroring the renderer's `instance_title` input).
    fn instance_node(name: &str, instance_index: usize, section_index: usize) -> GraphNode {
        GraphNode {
            id: NodeId::circuit(name, instance_index),
            kind: NodeKind::Circuit,
            circuit: name.to_string(),
            instance_index,
            section_index,
        }
    }

    /// Synthetic graph with `count` nodes and the given (source, sink) pairs.
    fn make_graph(count: usize, edges: &[(usize, usize)]) -> Graph {
        let nodes: Vec<GraphNode> = (0..count).map(|i| node(&format!("n{i}"), i)).collect();
        let edges: Vec<GraphEdge> = edges
            .iter()
            .map(|&(s, t)| GraphEdge {
                cable: "_C".to_string(),
                source: nodes[s].id.clone(),
                sink: nodes[t].id.clone(),
            })
            .collect();
        Graph {
            nodes,
            edges,
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        }
    }

    /// Build the signal-flow graph for a fixture patch, mirroring
    /// `App::open_graph` (banner groups → clusters, shared default cost model).
    fn graph_from_fixture(name: &str) -> Graph {
        let patch = Patch::from_ini_file(Path::new(name)).unwrap();
        let clusters: Vec<Cluster> = patch
            .banner_groups
            .iter()
            .map(|g| Cluster {
                title: g.banner.as_deref().unwrap_or("(unnamed)").to_string(),
                section_range: g.section_range.clone(),
            })
            .collect();
        Graph::build_from_patch(
            &patch,
            &clusters,
            &CostModel::default(),
            &GraphOptions::default(),
        )
    }

    fn assert_finite_and_bounded(positions: &[(f32, f32)]) {
        assert!(!positions.is_empty());
        for (x, y) in positions {
            assert!(x.is_finite(), "x not finite: {x}");
            assert!(y.is_finite(), "y not finite: {y}");
            assert!(x.abs() < 1e6, "x out of bounds: {x}");
            assert!(y.abs() < 1e6, "y out of bounds: {y}");
        }
    }

    #[test]
    fn single_node_solves_to_finite_position() {
        let graph = make_graph(1, &[]);
        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(positions.len(), 1);
        assert_finite_and_bounded(&positions);
    }

    #[test]
    fn disconnected_graph_produces_finite_positions() {
        // Three independent components with no connecting edges.
        let graph = make_graph(6, &[(0, 1), (2, 3), (4, 5)]);
        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(positions.len(), 6);
        assert_finite_and_bounded(&positions);
    }

    #[test]
    fn cyclic_graph_produces_finite_positions() {
        // A closed loop; topological depth is bounded by the solver.
        let graph = make_graph(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(positions.len(), 4);
        assert_finite_and_bounded(&positions);
    }

    #[test]
    fn six_hundred_node_chain_is_finite_without_panicking() {
        // The worst-case scale from the design goals: a long chain.
        let mut edges = Vec::with_capacity(599);
        for i in 0..599 {
            edges.push((i, i + 1));
        }
        let graph = make_graph(600, &edges);
        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(positions.len(), 600);
        assert_finite_and_bounded(&positions);
    }

    #[test]
    fn same_input_produces_identical_positions() {
        // Determinism sanity (formal determinism tests are task 3.2).
        let mut edges = Vec::new();
        for i in 0..49 {
            edges.push((i, i + 1));
        }
        let graph = make_graph(50, &edges);
        let a = solve(&graph, &[], SolveKnobs::default());
        let b = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(a, b);
    }

    #[test]
    fn freeze_stability_repeated_queries_are_identical() {
        // After a solve, re-solving the unchanged graph yields identical
        // frozen positions (no drift across queries).
        let graph = make_graph(40, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 0)]);
        let first = solve(&graph, &[], SolveKnobs::default());
        let second = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(first, second);
    }

    #[test]
    fn tension_is_deterministic_per_value_and_changes_layout() {
        // Determinism contract per tension value: same graph + same tension →
        // identical frozen positions, while a different tension re-flows the
        // layout (spring stiffness is a real solver input, not a no-op). A
        // fan-out graph exercises it: the sinks sit off spring rest in the
        // layered seed, so higher tension pulls them measurably closer to the
        // source.
        let graph = make_graph(6, &[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
        let low = solve(
            &graph,
            &[],
            SolveKnobs {
                tension: TENSION_MIN,
                ..Default::default()
            },
        );
        let low_again = solve(
            &graph,
            &[],
            SolveKnobs {
                tension: TENSION_MIN,
                ..Default::default()
            },
        );
        assert_eq!(low, low_again, "same tension must reproduce the layout");
        let high = solve(
            &graph,
            &[],
            SolveKnobs {
                tension: TENSION_MAX,
                ..Default::default()
            },
        );
        assert_finite_and_bounded(&low);
        assert_finite_and_bounded(&high);
        assert_ne!(low, high, "different tension must change the frozen layout");
        // Higher tension pulls cable-connected nodes closer: the mean
        // edge length at TENSION_MAX is below the one at TENSION_MIN.
        let mean_edge_len = |positions: &[(f32, f32)]| {
            let total: f32 = graph
                .edges
                .iter()
                .map(|e| {
                    let s = graph.nodes.iter().position(|n| n.id == e.source).unwrap();
                    let t = graph.nodes.iter().position(|n| n.id == e.sink).unwrap();
                    dist(positions[s], positions[t])
                })
                .sum();
            total / graph.edges.len() as f32
        };
        assert!(
            mean_edge_len(&high) < mean_edge_len(&low),
            "higher tension should pull connected nodes closer"
        );
    }

    #[test]
    fn tension_clamps_are_bounded_and_finite() {
        // The adjustable range stays inside the solver's stable regime: every
        // tension in [TENSION_MIN, TENSION_MAX] converges to finite positions.
        let graph = make_graph(20, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 0)]);
        let mut t = TENSION_MIN;
        while t <= TENSION_MAX + 1e-6 {
            assert_finite_and_bounded(&solve(
                &graph,
                &[],
                SolveKnobs {
                    tension: t,
                    ..Default::default()
                },
            ));
            t += TENSION_STEP;
        }
    }

    #[test]
    fn local_resettle_leaves_distant_nodes_unmoved() {
        let graph = make_graph(
            12,
            &[(0, 1), (1, 2), (2, 3), (4, 5), (6, 7), (8, 9), (10, 11)],
        );
        let mut positions = solve(&graph, &[], SolveKnobs::default());

        // Teleport node 0 far away; distant nodes must stay put.
        let before = positions.clone();
        positions[0] = (10000.0, 10000.0);
        let found = local_resettle(
            &graph,
            &mut positions,
            &NodeId::circuit("n0", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            SolveKnobs::default(),
        );
        assert!(found);
        assert_finite_and_bounded(&positions);

        // Nodes far from the moved one (e.g. 6..12) are untouched.
        for i in 6..12 {
            assert_eq!(positions[i], before[i], "node {i} moved during re-settle");
        }
    }

    #[test]
    fn local_resettle_unknown_node_is_noop() {
        let graph = make_graph(3, &[(0, 1), (1, 2)]);
        let mut positions = solve(&graph, &[], SolveKnobs::default());
        let before = positions.clone();
        let found = local_resettle(
            &graph,
            &mut positions,
            &NodeId::circuit("nope", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            SolveKnobs::default(),
        );
        assert!(!found);
        assert_eq!(positions, before);
    }

    #[test]
    fn single_axis_seed_spreads_layers_horizontally() {
        // D1: the seed places nodes by topological depth on x (not by cluster
        // band on y), so a layered graph starts as a horizontal chain.
        let nodes = vec![node("n0", 0), node("n1", 1), node("n2", 2)];
        let clusters = vec![
            Cluster {
                title: "left".to_string(),
                section_range: 0..2,
            },
            Cluster {
                title: "right".to_string(),
                section_range: 2..3,
            },
        ];
        let graph = Graph {
            nodes,
            edges: vec![
                GraphEdge {
                    cable: "_C".to_string(),
                    source: NodeId::circuit("n0", 0),
                    sink: NodeId::circuit("n1", 0),
                },
                GraphEdge {
                    cable: "_C".to_string(),
                    source: NodeId::circuit("n1", 0),
                    sink: NodeId::circuit("n2", 0),
                },
            ],
            clusters,
            validation: vec![],
            ..Default::default()
        };
        let positions = solve(&graph, &[], SolveKnobs::default());
        // Sinks sit right of sources: x grows with topological depth.
        assert!(positions[2].0 > positions[0].0 + HORIZONTAL_SPACING * 0.5);
        assert!(positions[2].0 > positions[1].0 + HORIZONTAL_SPACING * 0.5);
        assert_finite_and_bounded(&positions);
    }

    /// Count edge crossings between adjacent layers for a given within-layer
    /// order. Two edges (u1→v1, u2→v2) with u1,u2 in one layer and v1,v2 in
    /// the next cross iff (order[u1] < order[u2]) != (order[v1] < order[v2]).
    fn count_crossings(graph: &Graph, order: &[usize], depth: &[usize]) -> usize {
        let index = node_index(graph);
        let edges = edge_pairs(graph, &index);
        let mut crossings = 0;
        for &(u1, v1) in &edges {
            for &(u2, v2) in &edges {
                if u1 == u2 && v1 == v2 {
                    continue;
                }
                // Only pairs spanning the same adjacent layer pair count.
                if depth[u1] != depth[u2] || depth[v1] != depth[v2] {
                    continue;
                }
                if depth[v1] != depth[u1] + 1 {
                    continue;
                }
                if (order[u1] < order[u2]) != (order[v1] < order[v2]) {
                    crossings += 1;
                }
            }
        }
        crossings / 2 // each unordered pair is counted twice
    }

    #[test]
    fn layered_seed_reduces_crossings_vs_file_order() {
        // Two layers with crossed edges: A→D and B→C cross under file order
        // (layer 0 = [A, B], layer 1 = [C, D]) but not under the barycenter
        // order (layer 1 re-orders to [D, C]).
        let graph = Graph {
            nodes: vec![node("A", 0), node("B", 1), node("C", 2), node("D", 3)],
            edges: vec![
                GraphEdge {
                    cable: "_X".to_string(),
                    source: NodeId::circuit("A", 0),
                    sink: NodeId::circuit("D", 0),
                },
                GraphEdge {
                    cable: "_Y".to_string(),
                    source: NodeId::circuit("B", 0),
                    sink: NodeId::circuit("C", 0),
                },
            ],
            ..Default::default()
        };
        let positions = seed_positions(&graph);
        let depth: Vec<usize> = positions
            .iter()
            .map(|(x, _)| (x / HORIZONTAL_SPACING).round() as usize)
            .collect();
        let order: Vec<usize> = positions
            .iter()
            .map(|(_, y)| (y / VERTICAL_SPACING).round() as usize)
            .collect();
        // File order: A,B in layer 0; C,D in layer 1.
        let file_order = vec![0usize, 1, 0, 1];
        let file_crossings = count_crossings(&graph, &file_order, &depth);
        let layered_crossings = count_crossings(&graph, &order, &depth);
        assert!(file_crossings >= 1, "fixture should cross under file order");
        assert!(
            layered_crossings < file_crossings,
            "barycenter order should reduce crossings: {layered_crossings} !< {file_crossings}"
        );
    }

    #[test]
    fn layered_seed_aligns_same_layer_nodes_into_columns() {
        // Nodes at the same topological depth share the same x (aligned
        // column), and coordinates land exactly on the GRID_SNAP grid.
        let graph = make_graph(6, &[(0, 2), (1, 2), (2, 3), (3, 4), (3, 5)]);
        let positions = seed_positions(&graph);
        assert_eq!(positions[0].0, positions[1].0, "sources share a column");
        assert_eq!(positions[4].0, positions[5].0, "sinks share a column");
        for (x, y) in &positions {
            let sx = (x / GRID_SNAP).round() * GRID_SNAP;
            let sy = (y / GRID_SNAP).round() * GRID_SNAP;
            assert!((sx - x).abs() < 1e-3, "x {x} off the {GRID_SNAP} grid");
            assert!((sy - y).abs() < 1e-3, "y {y} off the {GRID_SNAP} grid");
        }
    }

    #[test]
    fn layered_seed_is_deterministic() {
        let graph = make_graph(
            20,
            &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 0), (5, 6), (6, 7)],
        );
        assert_eq!(seed_positions(&graph), seed_positions(&graph));
    }

    /// Number of iterations a full `solve` would run before freezing (energy
    /// threshold or cap). Lets tests assert convergence "within the cap"
    /// deterministically instead of timing the solver.
    fn solve_iteration_count(graph: &Graph) -> usize {
        let mut positions = seed_positions(graph);
        run_iterations(
            graph,
            &mut positions,
            SOLVE_ITERATIONS,
            None,
            &[],
            LayoutMode::Force,
            SolveKnobs::default(),
        )
    }

    /// Number of iterations a local re-settle around `moved` would run before
    /// freezing. Mirrors `local_resettle` but surfaces the iteration count.
    fn resettle_iteration_count(
        graph: &Graph,
        positions: &mut [(f32, f32)],
        moved: &NodeId,
        radius: f32,
        iterations: usize,
    ) -> usize {
        let Some(center) = graph.nodes.iter().position(|n| &n.id == moved) else {
            return 0;
        };
        let c = positions[center];
        let active: Vec<bool> = (0..graph.nodes.len())
            .map(|i| dist(positions[i], c) <= radius)
            .collect();
        run_iterations(
            graph,
            positions,
            iterations,
            Some(&active),
            &[],
            LayoutMode::Force,
            SolveKnobs::default(),
        )
    }

    #[test]
    fn solve_converges_within_iteration_cap() {
        // D1: the solver is bounded by SOLVE_ITERATIONS and must always return a
        // frozen, finite layout. Graphs that keep re-energizing (a cycle) run
        // to the cap, which stops the iteration — never an unbounded loop.
        let mut edges = Vec::new();
        for i in 0..39 {
            edges.push((i, (i + 1) % 40));
        }
        let graph = make_graph(40, &edges); // cyclic: converges at the cap
        let count = solve_iteration_count(&graph);
        assert!(count <= SOLVE_ITERATIONS, "solve exceeded cap: {count}");
        assert!(count > 0);
        assert_finite_and_bounded(&solve(&graph, &[], SolveKnobs::default()));
    }

    #[test]
    fn solve_converges_by_energy_threshold_before_cap() {
        // A settling graph (a long chain) converges via the energy-threshold
        // freeze well before the cap, proving "convergence" is the solver
        // reaching rest, not merely the cap cutting off an unsettled layout.
        let mut edges = Vec::new();
        for i in 0..49 {
            edges.push((i, i + 1));
        }
        let graph = make_graph(50, &edges);
        let count = solve_iteration_count(&graph);
        assert!(
            count < SOLVE_ITERATIONS,
            "expected energy convergence below cap, took {count} (cap {SOLVE_ITERATIONS})"
        );
        assert!(count > 0);
        assert_finite_and_bounded(&solve(&graph, &[], SolveKnobs::default()));
    }

    #[test]
    fn frozen_positions_stable_across_repeated_solves() {
        // Once a solve freezes, re-solving the unchanged graph must return
        // bit-identical positions — no drift between queries (D1 freeze
        // stability).
        let graph = make_graph(
            48,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (4, 0),
                (5, 6),
                (6, 7),
                (7, 8),
                (8, 5),
                (10, 11),
                (11, 12),
            ],
        );
        let baseline = solve(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&baseline);
        for _ in 0..3 {
            assert_eq!(
                solve(&graph, &[], SolveKnobs::default()),
                baseline,
                "positions drifted across a repeated query"
            );
        }
    }

    #[test]
    fn same_input_yields_identical_positions_on_same_machine() {
        // D9: deterministic seed, no RNG — two solves of the identical graph
        // are bit-identical. Only guaranteed on the same machine because the
        // node-id hash uses fixed DefaultHasher keys per process.
        let graph = make_graph(
            64,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (0, 4),
                (4, 5),
                (5, 6),
                (6, 7),
                (8, 9),
                (9, 10),
                (10, 11),
                (11, 8),
                (8, 0),
                (12, 13),
                (13, 14),
            ],
        );
        let a = solve(&graph, &[], SolveKnobs::default());
        let b = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(a, b);
        assert_finite_and_bounded(&a);
    }

    #[test]
    fn local_resettle_is_deterministic_for_same_move() {
        // Determinism extends to the damped re-settle: the same move from the
        // same frozen layout must yield the same result (D9).
        let graph = make_graph(20, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0)]);
        let baseline = solve(&graph, &[], SolveKnobs::default());

        let mut a = baseline.clone();
        a[0] = (5000.0, 5000.0);
        local_resettle(
            &graph,
            &mut a,
            &NodeId::circuit("n0", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            SolveKnobs::default(),
        );

        let mut b = baseline.clone();
        b[0] = (5000.0, 5000.0);
        local_resettle(
            &graph,
            &mut b,
            &NodeId::circuit("n0", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            SolveKnobs::default(),
        );

        assert_eq!(a, b);
    }

    #[test]
    fn local_resettle_budget_is_below_full_solve_cap() {
        // The local re-settle is contractually cheaper than a full solve (D1);
        // this structural invariant underlies "terminates faster" without
        // relying on wall-clock timing. Enforced at compile time so a budget
        // regression fails the build.
        const {
            assert!(LOCAL_ITERATIONS < SOLVE_ITERATIONS);
        };
    }

    #[test]
    fn local_resettle_terminates_faster_than_full_solve() {
        // The local re-settle is contractually cheaper than a full solve (D1):
        // a graph whose full solve needs many iterations vs a resettle capped
        // at LOCAL_ITERATIONS around one node. A chain seeds at spring rest
        // length and converges in ~20 iters under the spring-dominant
        // constants (D2), so it would not exercise the budget gap; a star of
        // 40 same-layer nodes fanned 4680 units apart in y, collapsing into
        // one sink, still takes the full solve far past LOCAL_ITERATIONS.
        let mut edges = Vec::new();
        for i in 0..40 {
            edges.push((i, 40));
        }
        let graph = make_graph(41, &edges);

        let full_count = solve_iteration_count(&graph);
        assert!(
            full_count > LOCAL_ITERATIONS,
            "expected full solve to need more than {LOCAL_ITERATIONS} iters, got {full_count}"
        );

        let mut positions = solve(&graph, &[], SolveKnobs::default());
        let resettle_count = resettle_iteration_count(
            &graph,
            &mut positions,
            &NodeId::circuit("n0", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
        );
        assert!(resettle_count <= LOCAL_ITERATIONS);
        assert!(
            resettle_count < full_count,
            "re-settle {resettle_count} iters not below full solve {full_count}"
        );
    }

    #[test]
    fn local_resettle_leaves_distant_anchors_unmoved() {
        // Teleporting one node far away: every node beyond LOCAL_RADIUS acts
        // as a fixed anchor and stays exactly where it was — only the moved
        // node's neighbourhood re-settles.
        let graph = make_graph(
            14,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (4, 5),
                (6, 7),
                (8, 9),
                (10, 11),
                (12, 13),
            ],
        );
        let baseline = solve(&graph, &[], SolveKnobs::default());
        let mut positions = baseline.clone();
        positions[0] = (10000.0, 10000.0);
        let found = local_resettle(
            &graph,
            &mut positions,
            &NodeId::circuit("n0", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            SolveKnobs::default(),
        );
        assert!(found);
        assert_finite_and_bounded(&positions);
        // All other nodes are farther than LOCAL_RADIUS from the teleported
        // position, so none of them move.
        for i in 1..14 {
            assert_eq!(
                positions[i], baseline[i],
                "distant node {i} moved during re-settle"
            );
        }
        // The moved node itself settles somewhere new.
        assert_ne!(positions[0], (10000.0, 10000.0));
    }

    // ── task 1.1 rework: single-axis, spring dominance, cohesion, pins ──────

    fn bbox_span(positions: &[(f32, f32)]) -> (f32, f32) {
        let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
        for (x, y) in positions {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
        (max_x - min_x, max_y - min_y)
    }

    #[test]
    fn solve_converges_along_single_axis_not_vertical_stack() {
        // Spec scenario scale: 60 circuits as three parallel chains of 20 — a
        // layered DAG with several nodes per layer. The solver must converge
        // along the x-axis (a wide horizontal pipeline), not a vertical stack.
        let mut edges = Vec::new();
        for chain in 0..3 {
            for k in 0..19 {
                edges.push((chain * 20 + k, chain * 20 + k + 1));
            }
        }
        let graph = make_graph(60, &edges);
        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);
        let (w, h) = bbox_span(&positions);
        assert!(w > 1000.0, "layout underuses the canvas width: {w}");
        assert!(
            w > 3.0 * h,
            "layout is not single-axis: x-span {w} vs y-span {h}"
        );
    }

    #[test]
    fn cable_springs_keep_connected_circuits_nearer_than_unconnected() {
        // D2: one connected pair (0-1) plus two isolated nodes (2,3). Spring
        // attraction must dominate repulsion so the connected pair settles
        // nearer each other than any unconnected pairing.
        let graph = make_graph(4, &[(0, 1)]);
        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);
        let connected = dist(positions[0], positions[1]);
        let unconnected = [
            dist(positions[0], positions[2]),
            dist(positions[0], positions[3]),
            dist(positions[1], positions[2]),
            dist(positions[1], positions[3]),
            dist(positions[2], positions[3]),
        ];
        let min_unconnected = unconnected.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(
            connected < min_unconnected,
            "spring dominance failed: connected {connected} !< unconnected {min_unconnected}"
        );
    }

    #[test]
    fn cluster_members_cohere_around_their_centroid() {
        // D3: banner-group members attract toward their cluster centroid. Two
        // 3-member clusters with no edges: intra-cluster spread must stay well
        // below the inter-cluster gap — members cohere, clusters don't merge
        // into a stripe or collapse together.
        let nodes: Vec<GraphNode> = (0..6).map(|i| node(&format!("n{i}"), i)).collect();
        let clusters = vec![
            Cluster {
                title: "A".to_string(),
                section_range: 0..3,
            },
            Cluster {
                title: "B".to_string(),
                section_range: 3..6,
            },
        ];
        let graph = Graph {
            nodes,
            edges: vec![],
            clusters,
            validation: vec![],
            ..Default::default()
        };
        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);
        let intra = [
            dist(positions[0], positions[1]),
            dist(positions[0], positions[2]),
            dist(positions[1], positions[2]),
            dist(positions[3], positions[4]),
            dist(positions[3], positions[5]),
            dist(positions[4], positions[5]),
        ];
        let max_intra = intra.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let inter = [
            dist(positions[0], positions[3]),
            dist(positions[0], positions[4]),
            dist(positions[0], positions[5]),
            dist(positions[1], positions[3]),
            dist(positions[1], positions[4]),
            dist(positions[1], positions[5]),
            dist(positions[2], positions[3]),
            dist(positions[2], positions[4]),
            dist(positions[2], positions[5]),
        ];
        let min_inter = inter.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(
            max_intra < min_inter,
            "cohesion failed: intra spread {max_intra} !< inter gap {min_inter}"
        );
    }

    #[test]
    fn pinned_nodes_never_move_during_solve() {
        // Pinned indices are fixed anchors: they stay exactly at their seed
        // position while every other node still settles under the forces.
        let graph = make_graph(10, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0)]);
        let seed = seed_positions(&graph);
        let positions = solve(&graph, &[0, 3], SolveKnobs::default());
        assert_finite_and_bounded(&positions);
        assert_eq!(positions[0], seed[0], "pinned node 0 moved");
        assert_eq!(positions[3], seed[3], "pinned node 3 moved");
        // Unpinned nodes still settle away from their seed (forces act on them).
        let mut any_unpinned_moved = false;
        for i in 0..10 {
            if i == 0 || i == 3 {
                continue;
            }
            if positions[i] != seed[i] {
                any_unpinned_moved = true;
                break;
            }
        }
        assert!(any_unpinned_moved, "unpinned nodes did not settle");
    }

    #[test]
    fn pinned_node_never_moves_during_local_resettle() {
        // Drag-to-place semantics: the caller drops a node, then re-settles
        // locally. A pinned node (here the moved one, and separately a pinned
        // neighbor) must stay exactly where it sits while neighbors pull.
        let graph = make_graph(
            12,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 8),
                (8, 9),
                (9, 10),
                (10, 11),
            ],
        );
        let baseline = solve(&graph, &[], SolveKnobs::default());

        // Case A: the moved node itself is pinned at its drop position.
        let mut a = baseline.clone();
        let drop = (baseline[0].0 + 30.0, baseline[0].1 + 10.0);
        a[0] = drop;
        let found = local_resettle(
            &graph,
            &mut a,
            &NodeId::circuit("n0", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[0],
            SolveKnobs::default(),
        );
        assert!(found);
        assert_eq!(a[0], drop, "pinned moved node must stay at its drop");
        // Its unpinned neighbour settles toward the new position.
        assert_ne!(a[1], baseline[1], "neighbour should re-settle");

        // Case B: a pinned neighbour never moves even as the moved node pulls.
        let mut b = baseline.clone();
        b[0] = drop;
        let found = local_resettle(
            &graph,
            &mut b,
            &NodeId::circuit("n0", 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[1],
            SolveKnobs::default(),
        );
        assert!(found);
        assert_eq!(b[1], baseline[1], "pinned neighbour must not move");
        assert_ne!(b[0], baseline[0], "unpinned moved node settles elsewhere");
    }

    #[test]
    fn empty_pin_slice_and_out_of_range_pins_behave_as_before() {
        // `&[]` is the pre-rework unpinned behavior (deterministic, all nodes
        // free); out-of-range pin indices are ignored defensively, never panic.
        let graph = make_graph(
            30,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (4, 5),
                (5, 6),
                (6, 7),
                (8, 9),
            ],
        );
        let free = solve(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&free);
        assert_eq!(
            solve(&graph, &[], SolveKnobs::default()),
            free,
            "&[] must be deterministic"
        );
        assert_eq!(
            solve(&graph, &[999, 1000, usize::MAX], SolveKnobs::default()),
            free,
            "out-of-range pins must be ignored"
        );
    }

    #[test]
    fn real_patch_solve_settles_quickly() {
        // A real patch (own_buttons.ini) must stay within the bounded refinement
        // budget: the layered seed is the primary arrangement (design gv5), so
        // the relaxation is a short refinement, not a full energy convergence.
        // The energy-threshold early exit still fires when the graph settles
        // sooner (e.g. cable_banner_combos.ini converges well under the cap).
        let graph = graph_from_fixture("fixtures/own_buttons.ini");
        assert!(graph.nodes.len() >= 10, "fixture must be non-trivial");
        let count = solve_iteration_count(&graph);
        assert!(
            count <= SOLVE_ITERATIONS,
            "real patch solve took {count} iterations (budget {SOLVE_ITERATIONS})"
        );
        assert!(count > 0);
        assert_finite_and_bounded(&solve(&graph, &[], SolveKnobs::default()));
    }

    #[test]
    fn real_patch_local_resettle_settles_quickly() {
        // Dragging a node on a real patch re-settles the local neighborhood
        // within the local budget, so the interactive drag stays bounded and
        // responsive. Asserted via the iteration count, not wall-clock timing:
        // the re-settle is capped at LOCAL_ITERATIONS and deterministic.
        let graph = graph_from_fixture("fixtures/own_buttons.ini");
        let mut positions = solve(&graph, &[], SolveKnobs::default());
        let moved = graph.nodes[0].id.clone();
        positions[0] = (positions[0].0 + 60.0, positions[0].1 + 40.0);
        let count = resettle_iteration_count(
            &graph,
            &mut positions,
            &moved,
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
        );
        assert!(
            count <= LOCAL_ITERATIONS,
            "real patch re-settle took {count} iterations (cap {LOCAL_ITERATIONS})"
        );
        assert!(count > 0);
        assert_finite_and_bounded(&positions);
    }

    // ── graph-column-layout: width-aware column arrangement (task 1.1) ─────

    #[test]
    fn columns_arrange_by_dense_normalized_depth() {
        // A chain: depth 0..4 dense-normalizes to columns 0..4; each column is
        // a block of the member width (20) plus the horizontal gap, and each
        // single-member column stacks at slot 0 (y = 0).
        let graph = make_graph(5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        let widths = vec![20.0; 5];
        let positions = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        assert_eq!(positions.len(), 5);
        for (i, (x, y)) in positions.iter().enumerate() {
            assert_eq!(*y, 0.0, "node {i} should sit on slot 0");
            let expected = i as f32 * (20.0 + HORIZONTAL_SPACING) + 10.0;
            assert_eq!(*x, expected, "node {i} off its block center");
        }
    }

    #[test]
    fn columns_are_deterministic() {
        // Determinism contract (design D9): same graph + same widths → identical
        // positions. Includes cycles (bounded depth growth) and fractional
        // widths that exercise the width-aware path.
        let graph = make_graph(
            24,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (4, 0),
                (0, 5),
                (5, 6),
                (6, 7),
                (7, 8),
                (9, 10),
                (10, 11),
                (11, 12),
                (12, 9),
                (13, 14),
                (14, 15),
                (15, 16),
                (16, 17),
                (17, 18),
                (18, 19),
                (20, 21),
                (21, 22),
                (22, 23),
            ],
        );
        let widths: Vec<f32> = (0..24).map(|i| 30.0 + (i % 5) as f32 * 7.5).collect();
        let a = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        let b = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        assert_eq!(a, b);
        assert_finite_and_bounded(&a);
    }

    #[test]
    fn column_placement_snaps_to_grid() {
        // GRID_SNAP contract: every coordinate lands on the layout grid even
        // with fractional widths.
        let graph = make_graph(
            12,
            &[
                (0, 3),
                (1, 3),
                (2, 3),
                (3, 6),
                (4, 6),
                (5, 6),
                (6, 9),
                (7, 9),
                (8, 9),
                (9, 10),
                (10, 11),
            ],
        );
        let widths: Vec<f32> = (0..12).map(|i| 17.5 + i as f32 * 1.3).collect();
        for (x, y) in solve_columns(&graph, &widths, LayoutOrdering::Strict) {
            let sx = (x / GRID_SNAP).round() * GRID_SNAP;
            let sy = (y / GRID_SNAP).round() * GRID_SNAP;
            assert!((sx - x).abs() < 1e-3, "x {x} off the {GRID_SNAP} grid");
            assert!((sy - y).abs() < 1e-3, "y {y} off the {GRID_SNAP} grid");
        }
    }

    // ── sections-black-hole-solver: slice index computation (task 1.2) ─────

    #[test]
    fn slice_index_matches_patch_order_for_circuits() {
        // Build a graph with 3 circuits (section_index 0, 1, 2), 1 controller,
        // and 1 output jack. Circuit nodes should get slice = section_index.
        // Controller gets left outer slice (0), output jack gets right outer
        // slice (max_circuit_slice + 1 = 3).
        let nodes = vec![
            node("circuit_a", 0), // index 0: circuit, section_index 0
            node("circuit_b", 1), // index 1: circuit, section_index 1
            node("circuit_c", 2), // index 2: circuit, section_index 2
            GraphNode {
                id: NodeId::Controller("ctrl1".to_string(), 0),
                kind: NodeKind::Controller,
                circuit: "ctrl1".to_string(),
                instance_index: 0,
                section_index: 0,
            }, // index 3: controller
            GraphNode {
                id: NodeId::Jack("OUT1".to_string()),
                kind: NodeKind::OutputJack,
                circuit: "OUT1".to_string(),
                instance_index: 0,
                section_index: 0,
            }, // index 4: output jack
        ];
        let graph = Graph {
            nodes,
            edges: vec![],
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        };
        let slice = slice_index(&graph);
        assert_eq!(slice.len(), 5);
        // Circuits: slice == section_index (patch order)
        assert_eq!(slice[0], 0, "circuit_a at section_index 0");
        assert_eq!(slice[1], 1, "circuit_b at section_index 1");
        assert_eq!(slice[2], 2, "circuit_c at section_index 2");
        // Controller: fixed left outer slice (0)
        assert_eq!(slice[3], 0, "controller gets left outer slice");
        // Output jack: fixed right outer slice (max_circuit_slice + 1 = 3)
        assert_eq!(slice[4], 3, "output jack gets right outer slice");
    }

    #[test]
    fn slice_index_handles_input_jack() {
        // Input jack should also get left outer slice (0) like controller
        let nodes = vec![
            node("circuit_a", 0),
            node("circuit_b", 5), // non-contiguous section_index
            GraphNode {
                id: NodeId::Jack("IN1".to_string()),
                kind: NodeKind::InputJack,
                circuit: "IN1".to_string(),
                instance_index: 0,
                section_index: 0,
            },
        ];
        let graph = Graph {
            nodes,
            edges: vec![],
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        };
        let slice = slice_index(&graph);
        assert_eq!(slice[0], 0, "circuit_a at section_index 0");
        assert_eq!(slice[1], 5, "circuit_b at section_index 5");
        assert_eq!(slice[2], 0, "input jack gets left outer slice");
    }

    #[test]
    fn slice_index_all_circuits_no_outer_nodes() {
        // Graph with only circuits: slices are just section_index values
        let graph = make_graph(4, &[(0, 1), (1, 2), (2, 3)]);
        let slice = slice_index(&graph);
        assert_eq!(slice, vec![0, 1, 2, 3]);
    }

    // ── sections-black-hole-solver: section-hole anchor computation (task 1.3) ──

    #[test]
    fn compute_section_holes_two_banner_groups() {
        // Build a graph with 2 banner groups (clusters):
        // Cluster 0: circuits at section_index 0, 1 (nodes 0, 1)
        // Cluster 1: circuits at section_index 3, 4, 5 (nodes 3, 4, 5)
        // Nodes 2 and 6 are circuits not in any cluster (None expected)
        // Node 7 is a controller (in cluster 0 via section_index 0, but not a circuit)
        let nodes = vec![
            node("c0", 0), // index 0: circuit, section_index 0, cluster 0
            node("c1", 1), // index 1: circuit, section_index 1, cluster 0
            node("c2", 2), // index 2: circuit, section_index 2, no cluster
            node("c3", 3), // index 3: circuit, section_index 3, cluster 1
            node("c4", 4), // index 4: circuit, section_index 4, cluster 1
            node("c5", 5), // index 5: circuit, section_index 5, cluster 1
            node("c6", 6), // index 6: circuit, section_index 6, no cluster
            GraphNode {
                id: NodeId::Controller("ctrl1".to_string(), 0),
                kind: NodeKind::Controller,
                circuit: "ctrl1".to_string(),
                instance_index: 0,
                section_index: 0,
            }, // index 7: controller, section_index 0, cluster 0
        ];
        let clusters = vec![
            Cluster {
                title: "group_a".to_string(),
                section_range: 0..2, // covers section_index 0, 1
            },
            Cluster {
                title: "group_b".to_string(),
                section_range: 3..6, // covers section_index 3, 4, 5
            },
        ];
        let graph = Graph {
            nodes,
            edges: vec![],
            clusters,
            validation: vec![],
            ..Default::default()
        };

        let slice = slice_index(&graph);
        let holes = compute_section_holes(&graph, &slice);

        assert_eq!(holes.len(), 8);

        // Cluster 0: circuits at indices 0, 1 with slices 0, 1
        // Mean slice = (0 + 1) / 2 = 0.5
        // Anchor = (0.5 * 80.0, 0.0) = (40.0, 0.0)
        let expected_a = Some((0.5 * HORIZONTAL_SPACING, 0.0));
        assert_eq!(holes[0], expected_a, "circuit 0 in cluster 0");
        assert_eq!(holes[1], expected_a, "circuit 1 in cluster 0");
        // Controller at index 7 has section_index in cluster 0 range but is not a circuit
        // so it should not get the section-hole anchor
        assert_eq!(holes[7], None, "controller not a circuit -> no anchor");

        // Cluster 1: circuits at indices 3, 4, 5 with slices 3, 4, 5
        // Mean slice = (3 + 4 + 5) / 3 = 4.0
        // Anchor = (4.0 * 80.0, 0.0) = (320.0, 0.0)
        let expected_b = Some((4.0 * HORIZONTAL_SPACING, 0.0));
        assert_eq!(holes[3], expected_b, "circuit 3 in cluster 1");
        assert_eq!(holes[4], expected_b, "circuit 4 in cluster 1");
        assert_eq!(holes[5], expected_b, "circuit 5 in cluster 1");

        // Nodes not in any cluster get None
        assert_eq!(holes[2], None, "circuit 2 not in any cluster");
        assert_eq!(holes[6], None, "circuit 6 not in any cluster");
    }

    #[test]
    fn compute_section_holes_empty_clusters_returns_all_none() {
        // Graph with no clusters: all nodes should get None
        let graph = make_graph(4, &[(0, 1), (1, 2), (2, 3)]);
        let slice = slice_index(&graph);
        let holes = compute_section_holes(&graph, &slice);
        assert_eq!(holes, vec![None, None, None, None]);
    }

    #[test]
    fn compute_section_holes_cluster_with_no_circuits_returns_none_for_members() {
        // Cluster that only contains non-circuit nodes (e.g. only controller)
        // should result in None for those members (no circuits to average)
        let nodes = vec![
            GraphNode {
                id: NodeId::Controller("ctrl1".to_string(), 0),
                kind: NodeKind::Controller,
                circuit: "ctrl1".to_string(),
                instance_index: 0,
                section_index: 0,
            },
            GraphNode {
                id: NodeId::Jack("IN1".to_string()),
                kind: NodeKind::InputJack,
                circuit: "IN1".to_string(),
                instance_index: 0,
                section_index: 0,
            },
        ];
        let clusters = vec![Cluster {
            title: "controllers".to_string(),
            section_range: 0..1,
        }];
        let graph = Graph {
            nodes,
            edges: vec![],
            clusters,
            validation: vec![],
            ..Default::default()
        };

        let slice = slice_index(&graph);
        let holes = compute_section_holes(&graph, &slice);
        // No circuits in cluster, so members get None
        assert_eq!(holes, vec![None, None]);
    }

    // ── sections-black-hole-solver: sections-mode solver (task 1.4) ──────────

    #[test]
    fn sections_mode_two_clusters_settle_around_lane_slot_anchors() {
        // Band-anchor contract (task 3.8, retargeted by 3.9): a banner group's
        // fixed black hole is the mean **seed target** of its member circuits —
        // one anchor per (group, row) band — and the pull a member feels is
        // bounded by [`SECTIONS_TETHER`]. Members therefore compact *inside* their
        // own lane band instead of being dragged along one global
        // `mean_slice * HORIZONTAL_SPACING` axis, and the compaction can never
        // squeeze a member out of the space its lane reserved.
        //
        // Build a graph with 2 banner groups (clusters), each with 3 circuits:
        // Cluster 0: circuits at section_index 0, 1, 2 (nodes 0, 1, 2)
        // Cluster 1: circuits at section_index 4, 5, 6 (nodes 4, 5, 6)
        // Node 3 is a circuit not in any cluster (section_index 3)
        // Node 7 is a controller in cluster 0's range (not an anchor member)
        // Node 8 is an output jack
        //
        // Hand-derived lane geometry (lane width = widest member's capped
        // estimated width; title widths are chars × 8 + 16): lane 0 holds c0
        // (32) + ctrl1 (56) -> 56, lanes 1..6 hold one 32-wide circuit each,
        // lane 7 holds OUT1 (48). With SECTIONS_LANE_GAP = 40 the slots come out
        // at x = 0, 96, 168, 240, 312, 384, 456, 528 (one row, slot y = 0).
        let nodes = vec![
            node("c0", 0), // index 0: circuit, section_index 0, cluster 0
            node("c1", 1), // index 1: circuit, section_index 1, cluster 0
            node("c2", 2), // index 2: circuit, section_index 2, cluster 0
            node("c3", 3), // index 3: circuit, section_index 3, no cluster
            node("c4", 4), // index 4: circuit, section_index 4, cluster 1
            node("c5", 5), // index 5: circuit, section_index 5, cluster 1
            node("c6", 6), // index 6: circuit, section_index 6, cluster 1
            GraphNode {
                id: NodeId::Controller("ctrl1".to_string(), 0),
                kind: NodeKind::Controller,
                circuit: "ctrl1".to_string(),
                instance_index: 0,
                section_index: 0,
            }, // index 7: controller, section_index 0, cluster 0 range
            GraphNode {
                id: NodeId::Jack("OUT1".to_string()),
                kind: NodeKind::OutputJack,
                circuit: "OUT1".to_string(),
                instance_index: 0,
                section_index: 10, // outside any cluster range
            }, // index 8: output jack
        ];
        let clusters = vec![
            Cluster {
                title: "group_a".to_string(),
                section_range: 0..3, // covers section_index 0, 1, 2
            },
            Cluster {
                title: "group_b".to_string(),
                section_range: 4..7, // covers section_index 4, 5, 6
            },
        ];
        let graph = Graph {
            nodes,
            edges: vec![], // No edges — pure clustering by section holes
            clusters,
            validation: vec![],
            ..Default::default()
        };

        // Pin the lane geometry and the anchors it produces before solving.
        // One row holds every lane, and each lane holds a single node, so a
        // member's seed target *is* its lane slot and the per-(group, row)
        // anchor collapses to the old single mean slot — the values below stay
        // the hand-derived hand-over from task 3.8.
        let seed = sections_seed(&graph);
        assert_eq!(
            seed.lanes.slot,
            vec![
                (0.0, 0.0),
                (96.0, 0.0),
                (168.0, 0.0),
                (240.0, 0.0),
                (312.0, 0.0),
                (384.0, 0.0),
                (456.0, 0.0),
                (528.0, 0.0)
            ]
        );
        let anchors = section_hole_anchors_lanes(&graph, &seed);
        let anchor_a = (0.0 + 96.0 + 168.0) / 3.0; // mean slot x of nodes 0, 1, 2
        let anchor_b = (312.0 + 384.0 + 456.0) / 3.0; // mean slot x of nodes 4, 5, 6
        assert_eq!(anchors[0], Some((anchor_a, 0.0)));
        assert_eq!(anchors[2], Some((anchor_a, 0.0)));
        assert_eq!(anchors[4], Some((anchor_b, 0.0)));
        assert_eq!(anchors[6], Some((anchor_b, 0.0)));
        // Non-members: the unclustered circuit, the controller (not a circuit
        // member), and the output jack stay unanchored.
        assert_eq!(anchors[3], None);
        assert_eq!(anchors[7], None);
        assert_eq!(anchors[8], None);

        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);

        // Cluster 0's circuit members settle around their anchor. Tolerance
        // HORIZONTAL_SPACING * 0.5 = 40 world units; observed drift 0.0 (the
        // anchor pull is bounded by SECTIONS_TETHER = 60, so the three members
        // land symmetrically around the anchor at x = 52.8 / 91.2 / 120.0, and
        // their mean is the anchor exactly).
        let cluster_a_x: f32 = [0, 1, 2].iter().map(|&i| positions[i].0).sum::<f32>() / 3.0;
        let dx_a = (cluster_a_x - anchor_a).abs();
        assert!(
            dx_a < HORIZONTAL_SPACING * 0.5,
            "cluster 0 mean x={cluster_a_x} not near its lane-slot anchor x={anchor_a} (dx={dx_a})"
        );

        // Cluster 1: same check; observed drift 0.0 as well (its members are two
        // lane pitches apart, so the bounded pull moves them symmetrically about
        // the anchor: 355.2 / 384.0 / 412.8).
        let cluster_b_x: f32 = [4, 5, 6].iter().map(|&i| positions[i].0).sum::<f32>() / 3.0;
        let dx_b = (cluster_b_x - anchor_b).abs();
        assert!(
            dx_b < HORIZONTAL_SPACING * 0.5,
            "cluster 1 mean x={cluster_b_x} not near its lane-slot anchor x={anchor_b} (dx={dx_b})"
        );

        // Non-clustered circuit (index 3) has no anchor: the slice spring holds it
        // on its own lane slot (240), between the two cluster anchors.
        let c3_x = positions[3].0;
        assert!(
            (c3_x - 240.0).abs() <= SECTIONS_LANE_GAP / 4.0,
            "unclustered circuit 3 x={c3_x} must stay on its lane slot (240)"
        );
        assert!(
            c3_x > cluster_a_x && c3_x < cluster_b_x,
            "non-clustered circuit 3 x={c3_x} should be between clusters ({cluster_a_x}, {cluster_b_x})"
        );

        // Output jack (index 8) is unanchored and sits on the right outer lane
        // slot (528), right of cluster 1.
        let jack_x = positions[8].0;
        assert!(
            jack_x > cluster_b_x,
            "output jack x={jack_x} should be right of cluster 1 ({cluster_b_x})"
        );

        // Bounded-compaction contract (task 3.9): the anchor pull is capped by
        // [`SECTIONS_TETHER`], so the x-compaction that used to force a diagonal
        // fallback no longer happens — every member stays in its own lane's row
        // (slot y = 0) and within the tether of its own lane slot. Before 3.9 the
        // unbounded hole spring squeezed cluster 0's members to ~45-unit
        // x-neighbours and size-aware repulsion had to push them 29.14 units apart
        // vertically; now the observed y spread is 0.0 for both clusters (all six
        // members stay exactly on row 0's edge) and the worst x offset from a
        // lane slot is 52.8 (cluster 0) / 43.2 (cluster 1), both inside the
        // 60-unit tether.
        let y_spread = |indices: [usize; 3]| {
            let ys: Vec<f32> = indices.iter().map(|&i| positions[i].1).collect();
            ys.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - ys.iter().copied().fold(f32::INFINITY, f32::min)
        };
        let (y_spread_a, y_spread_b) = (y_spread([0, 1, 2]), y_spread([4, 5, 6]));
        for (name, spread) in [("cluster 0", y_spread_a), ("cluster 1", y_spread_b)] {
            assert!(
                spread <= GRID_SNAP,
                "{name} must stay on its lanes' row (no diagonal fallback): y spread {spread}"
            );
        }
        for (i, slot) in [
            (0usize, 0.0f32),
            (1, 96.0),
            (2, 168.0),
            (4, 312.0),
            (5, 384.0),
            (6, 456.0),
        ] {
            let offset = (positions[i].0 - slot).abs();
            assert!(
                offset <= SECTIONS_TETHER,
                "member {i} x={} left its lane slot {slot} by {offset} > SECTIONS_TETHER = {SECTIONS_TETHER}",
                positions[i].0
            );
            assert!(
                positions[i].1.abs() <= GRID_SNAP,
                "member {i} y={} left row 0",
                positions[i].1
            );
        }

        // The two clusters are well separated horizontally (observed gap 197.93).
        let cluster_a_max_x = [0, 1, 2]
            .iter()
            .map(|&i| positions[i].0)
            .fold(f32::NEG_INFINITY, f32::max);
        let cluster_b_min_x = [4, 5, 6]
            .iter()
            .map(|&i| positions[i].0)
            .fold(f32::INFINITY, f32::min);
        let inter_cluster_gap = cluster_b_min_x - cluster_a_max_x;
        assert!(
            inter_cluster_gap > HORIZONTAL_SPACING * 2.0,
            "clusters should be well separated horizontally (gap={inter_cluster_gap})"
        );
    }

    #[test]
    fn sections_mode_deterministic() {
        // Same graph + same force constants -> identical positions (determinism).
        // The lane contract (task 3.8) keeps that guarantee: lane assignment,
        // lane widths, the wrap walk, the lane-slot anchors, and the packing are
        // all pure functions of the graph — no RNG and no hash iteration order —
        // so the wrapped seed and every force target are reproducible.
        let nodes = vec![node("c0", 0), node("c1", 1), node("c2", 4), node("c3", 5)];
        let clusters = vec![
            Cluster {
                title: "a".to_string(),
                section_range: 0..2,
            },
            Cluster {
                title: "b".to_string(),
                section_range: 4..6,
            },
        ];
        let graph = Graph {
            nodes,
            edges: vec![],
            clusters,
            validation: vec![],
            ..Default::default()
        };
        let a = solve_sections(&graph, &[], SolveKnobs::default());
        let b = solve_sections(&graph, &[], SolveKnobs::default());
        assert_eq!(a, b, "sections mode must be deterministic");
        assert_finite_and_bounded(&a);
    }

    #[test]
    fn sections_mode_with_edges_respects_cable_springs() {
        // Two clusters with internal edges: cable springs should keep connected
        // nodes closer than unconnected ones, while hole springs keep clusters
        // separated. Under the lane contract (task 3.8) the circuits sit on lanes
        // 0, 1, 4, 5 — lane width 46 each (32 title + 1 port marker) → slots 0,
        // 86, 332, 418 with the two empty lanes in between.
        let nodes = vec![
            node("c0", 0), // cluster 0
            node("c1", 1), // cluster 0
            node("c2", 4), // cluster 1
            node("c3", 5), // cluster 1
        ];
        let clusters = vec![
            Cluster {
                title: "a".to_string(),
                section_range: 0..2,
            },
            Cluster {
                title: "b".to_string(),
                section_range: 4..6,
            },
        ];
        let edges = vec![
            GraphEdge {
                cable: "_C1".to_string(),
                source: NodeId::circuit("c0", 0),
                sink: NodeId::circuit("c1", 0),
            },
            GraphEdge {
                cable: "_C2".to_string(),
                source: NodeId::circuit("c2", 0),
                sink: NodeId::circuit("c3", 0),
            },
        ];
        let graph = Graph {
            nodes,
            edges,
            clusters,
            validation: vec![],
            ..Default::default()
        };

        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);
        assert_eq!(
            sections_lanes(&graph).slot,
            vec![
                (0.0, 0.0),
                (86.0, 0.0),
                (172.0, 0.0),
                (252.0, 0.0),
                (332.0, 0.0),
                (418.0, 0.0)
            ]
        );

        // Connected pairs should be closer than the inter-cluster gap, and the
        // cable spring should pull them below their own lane pitch (86): observed
        // 68.04 for both pairs.
        let connected_a = dist(positions[0], positions[1]);
        let connected_b = dist(positions[2], positions[3]);
        let cluster_a_max_x = positions[0].0.max(positions[1].0);
        let cluster_b_min_x = positions[2].0.min(positions[3].0);
        let inter_gap = cluster_b_min_x - cluster_a_max_x;

        assert!(
            connected_a < 86.0 && connected_b < 86.0,
            "cable springs should pull each connected pair below its lane pitch: \
             {connected_a} and {connected_b}"
        );
        assert!(
            connected_a < inter_gap,
            "connected pair in cluster 0 should be closer than inter-cluster gap"
        );
        assert!(
            connected_b < inter_gap,
            "connected pair in cluster 1 should be closer than inter-cluster gap"
        );
    }

    #[test]
    fn sections_mode_force_params_change_member_spread_near_anchor() {
        // sections-black-hole-solver 3.0: the runtime slice/section stiffnesses
        // must actually reach the solver. Solves one banner group whose circuits
        // span lanes 0, 2, 4, 6 twice with the same slice_k but a low vs a high
        // section_k. Under the band-anchor contract (task 3.9) the anchor is the
        // mean seed target of those members — the slots are 0, 152, 304, 456
        // (each 32-wide circuit lane followed by an empty lane credited
        // `SECTIONS_LANE_MIN_W` = 40), so the anchor x is 228. The section-hole
        // spring is the only x-force that pulls members together, so high
        // section_k must yield a measurably tighter x-cluster around the anchor
        // than low section_k — within the bound `SECTIONS_TETHER` puts on it.
        let nodes = vec![node("c0", 0), node("c1", 2), node("c2", 4), node("c3", 6)];
        let clusters = vec![Cluster {
            title: "group_a".to_string(),
            section_range: 0..7,
        }];
        let graph = Graph {
            nodes,
            edges: vec![],
            clusters,
            validation: vec![],
            ..Default::default()
        };

        // The anchor the solver actually uses: the mean seed target of the
        // members. Every node here is alone in its lane and every lane is in row
        // 0, so the per-(group, row) anchor is again the members' mean slot.
        let seed = sections_seed(&graph);
        let anchors = section_hole_anchors_lanes(&graph, &seed);
        let anchor_x = anchors[0].expect("cluster member is anchored").0;
        assert_eq!(anchor_x, (0.0 + 152.0 + 304.0 + 456.0) / 4.0);
        assert!(
            anchors.iter().all(|a| a.map(|p| p.0) == Some(anchor_x)),
            "in one row every member shares one band anchor"
        );
        let spread = |positions: &[(f32, f32)]| {
            positions
                .iter()
                .map(|(x, _)| (x - anchor_x).abs())
                .fold(0.0f32, f32::max)
        };

        let low_section_k = SECTION_HOLE_K / 20.0; // hole spring barely felt
        let high_section_k = DEFAULT_SECTION_HOLE_K * 20.0; // strongly anchored

        let low = solve_sections(
            &graph,
            &[],
            SolveKnobs {
                section_k: low_section_k,
                ..Default::default()
            },
        );
        let high = solve_sections(
            &graph,
            &[],
            SolveKnobs {
                section_k: high_section_k,
                ..Default::default()
            },
        );
        assert_finite_and_bounded(&low);
        assert_finite_and_bounded(&high);

        // Bounded-compaction contract (task 3.9): an anchor pull can move a
        // member toward its band anchor by at most SECTIONS_TETHER, so a
        // saturated high section_k lands exactly on
        // `max|target − anchor| − SECTIONS_TETHER` = 228 − 60 = 168 (observed
        // 168.0), while a barely-felt hole spring leaves the members near their
        // own slots (observed 215.10 for its 12.9 units of tightening). High
        // section_k must therefore be measurably tighter — but boundedly so,
        // which is the property that stops the tightening from collapsing the
        // lanes into each other.
        let (spread_low, spread_high) = (spread(&low), spread(&high));
        assert!(
            spread_high < spread_low,
            "high section_k must tighten the x-cluster around the anchor: \
             spread_low={spread_low} spread_high={spread_high}"
        );
        assert!(
            (spread_high - (228.0 - SECTIONS_TETHER)).abs() <= 1.0,
            "a saturated anchor pull must land on max|target − anchor| − \
             SECTIONS_TETHER = {}, got spread_high={spread_high}",
            228.0 - SECTIONS_TETHER
        );
        assert!(
            spread_low - spread_high <= SECTIONS_TETHER,
            "the tightening a high section_k buys is bounded by the tether: \
             {spread_low} − {spread_high} > {SECTIONS_TETHER}"
        );
        assert_ne!(
            low, high,
            "different section_k values must produce different layouts"
        );

        // Determinism holds per parameter triple.
        let low_again = solve_sections(
            &graph,
            &[],
            SolveKnobs {
                section_k: low_section_k,
                ..Default::default()
            },
        );
        let high_again = solve_sections(
            &graph,
            &[],
            SolveKnobs {
                section_k: high_section_k,
                ..Default::default()
            },
        );
        assert_eq!(low, low_again, "low section_k solve must be deterministic");
        assert_eq!(
            high, high_again,
            "high section_k solve must be deterministic"
        );
    }

    // ── graph-column-layout: pinned anchors on the column path (task 3.2) ──

    #[test]
    fn columns_pinned_node_keeps_position_rest_arrange_around() {
        // Design D6: a pinned node is a fixed-position anchor. Pin the middle
        // member of a column at slot 0's position: it stays there exactly and
        // the remaining column members dense-stack into the free slots below.
        let graph = make_graph(4, &[(0, 3), (1, 3), (2, 3)]);
        let widths = vec![20.0; 4];
        let positions =
            solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[(1, (10.0, 0.0))]);
        assert_eq!(
            positions,
            vec![(10.0, 120.0), (10.0, 0.0), (10.0, 240.0), (110.0, 0.0)],
            "pinned node holds slot 0 while the rest arrange around it"
        );
        assert_columns_do_not_overlap(&graph, &widths, &positions);
    }

    #[test]
    fn columns_pinned_solve_is_deterministic() {
        // Determinism contract (design D9) extends to the pin set: same graph,
        // widths, ordering, and anchors → identical positions.
        let graph = make_graph(6, &[(0, 5), (1, 5), (2, 5), (3, 5), (4, 5)]);
        let widths = vec![20.0; 6];
        let pins = [(1, (10.0, 120.0)), (3, (10.0, 360.0))];
        let a = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &pins);
        let b = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &pins);
        assert_eq!(a, b);
        assert_finite_and_bounded(&a);
    }

    #[test]
    fn columns_pinned_leaves_other_columns_untouched() {
        // Pinning one column rearranges only that column: the other columns'
        // positions are byte-identical to the unpinned solve.
        let graph = make_graph(4, &[(0, 3), (1, 3), (2, 3)]);
        let widths = vec![20.0; 4];
        let pinless = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        let pinned =
            solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[(1, (10.0, 0.0))]);
        assert_ne!(pinned[0], pinless[0], "the pinned column re-flows");
        assert_eq!(pinned[3], pinless[3], "the other column is untouched");
    }

    #[test]
    fn columns_pinned_empty_pin_set_matches_plain_solve() {
        // `solve_columns` delegates with an empty pin set: no-pin callers keep
        // the pre-task-3.2 arrangement exactly.
        let graph = make_graph(4, &[(0, 3), (1, 3), (2, 3)]);
        let widths = vec![20.0; 4];
        let plain = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        let pinned = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[]);
        assert_eq!(plain, pinned);
    }

    #[test]
    fn columns_pinned_multiple_anchors_reserve_their_slots() {
        // Two anchors in one column: each reserves its slot and the unpinned
        // members dense-stack into the remaining slots in rank order.
        let graph = make_graph(6, &[(0, 5), (1, 5), (2, 5), (3, 5), (4, 5)]);
        let widths = vec![20.0; 6];
        let positions = solve_columns_pinned(
            &graph,
            &widths,
            LayoutOrdering::Strict,
            &[(1, (10.0, 120.0)), (3, (10.0, 360.0))],
        );
        assert_eq!(
            positions,
            vec![
                (10.0, 0.0),
                (10.0, 120.0),
                (10.0, 240.0),
                (10.0, 360.0),
                (10.0, 480.0),
                (110.0, 0.0),
            ]
        );
        assert_columns_do_not_overlap(&graph, &widths, &positions);
    }

    #[test]
    fn columns_pinned_keeps_offgrid_position_exactly() {
        // D6 keeps the anchor verbatim, even off the slot grid: the pinned
        // node stays at its given y while unpinned members keep the slot grid.
        let graph = make_graph(4, &[(0, 3), (1, 3), (2, 3)]);
        let widths = vec![20.0; 4];
        let positions = solve_columns_pinned(
            &graph,
            &widths,
            LayoutOrdering::Strict,
            &[(1, (10.0, 10.0))],
        );
        assert_eq!(
            positions,
            vec![(10.0, 120.0), (10.0, 10.0), (10.0, 240.0), (110.0, 0.0)],
            "off-grid anchor kept exactly, others stay on the slot grid"
        );
    }

    /// No-overlap property (design D3), verified against the solver's own
    /// column assignment: nodes in the same column stack on distinct
    /// [`VERTICAL_SPACING`] slots (never sharing vertical space), and every
    /// node's horizontal extent stays inside its column block, so blocks (and
    /// the nodes in them) never overlap. Shared by the fractional-width,
    /// estimated-width, and mixed-kind tests so the property is checked the
    /// same way everywhere.
    fn assert_columns_do_not_overlap(graph: &Graph, widths: &[f32], positions: &[(f32, f32)]) {
        let n = graph.nodes.len();
        assert_eq!(positions.len(), n);
        let depth = circuit_depth(graph);
        let (cols, ncols) = assign_columns(graph, &depth);
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); ncols];
        for (i, &c) in cols.iter().enumerate() {
            members[c].push(i);
        }

        let mut block_span: Vec<(f32, f32)> = Vec::with_capacity(ncols);
        for (c, list) in members.iter().enumerate() {
            // Vertical: slots are ≥ VERTICAL_SPACING apart, so no two members
            // share vertical space.
            for a in 0..list.len() {
                for b in (a + 1)..list.len() {
                    let dy = (positions[list[a]].1 - positions[list[b]].1).abs();
                    assert!(
                        dy >= VERTICAL_SPACING,
                        "graph {n}: column {c} nodes {} and {} share vertical space (dy {dy})",
                        list[a],
                        list[b]
                    );
                }
            }
            // Horizontal: the block spans the members' extents (per-column
            // max width), so adjacent blocks keep a real gap.
            let left = list
                .iter()
                .map(|&i| positions[i].0 - widths[i] / 2.0)
                .fold(f32::INFINITY, f32::min);
            let right = list
                .iter()
                .map(|&i| positions[i].0 + widths[i] / 2.0)
                .fold(f32::NEG_INFINITY, f32::max);
            block_span.push((left, right));
        }
        for k in 0..ncols.saturating_sub(1) {
            let gap = block_span[k + 1].0 - block_span[k].1;
            assert!(
                gap >= HORIZONTAL_SPACING - 1e-3,
                "graph {n}: column {k} block overlaps column {} (gap {gap})",
                k + 1
            );
        }
    }

    #[test]
    fn column_nodes_never_share_vertical_space() {
        // No-overlap property (design D3), checked across graph shapes and
        // fractional widths.
        let mut sixty_chain = Vec::new();
        for chain in 0..3 {
            for k in 0..19 {
                sixty_chain.push((chain * 20 + k, chain * 20 + k + 1));
            }
        }
        let cases: Vec<(usize, Vec<(usize, usize)>)> = vec![
            (6, vec![(0, 2), (1, 2), (2, 3), (3, 4), (3, 5)]),
            (
                12,
                vec![
                    (0, 3),
                    (1, 3),
                    (2, 3),
                    (3, 6),
                    (4, 6),
                    (5, 6),
                    (6, 9),
                    (7, 9),
                    (8, 9),
                    (9, 10),
                    (10, 11),
                    (11, 9),
                ],
            ),
            (60, sixty_chain),
        ];
        for (n, edges) in cases {
            let graph = make_graph(n, &edges);
            let widths: Vec<f32> = (0..n).map(|i| 15.0 + (i % 7) as f32 * 6.3).collect();
            let positions = solve_columns(&graph, &widths, LayoutOrdering::Strict);
            assert_columns_do_not_overlap(&graph, &widths, &positions);
        }
    }

    #[test]
    fn column_pitch_grows_with_node_widths() {
        // Width-aware placement: wider members make wider blocks, so the block
        // pitch (and total span) grows with the widths — not a fixed constant.
        let graph = make_graph(6, &[(0, 2), (1, 2), (2, 3), (3, 4), (3, 5)]);
        let narrow = vec![10.0; 6];
        let wide = vec![200.0; 6];
        let span = |positions: &[(f32, f32)]| {
            let xs: Vec<f32> = positions.iter().map(|&(x, _)| x).collect();
            let min = xs.iter().copied().fold(f32::INFINITY, f32::min);
            let max = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            max - min
        };
        let narrow_span = span(&solve_columns(&graph, &narrow, LayoutOrdering::Strict));
        let wide_span = span(&solve_columns(&graph, &wide, LayoutOrdering::Strict));
        assert!(
            wide_span > narrow_span,
            "wide nodes should spread columns: {wide_span} !> {narrow_span}"
        );
    }

    // ── graph-column-layout: ordering switch (task 1.2) ─────────────────────

    #[test]
    fn both_orderings_assign_identical_columns() {
        // Task 1.2 contract: strict and barycenter produce the SAME column
        // assignment; only the within-column vertical order may differ. The
        // crossed A→D / B→C fixture (depth 0 = [A, B], depth 1 = [C, D]) keeps
        // both nodes of a layer in one column under either ordering.
        let graph = Graph {
            nodes: vec![node("A", 0), node("B", 1), node("C", 2), node("D", 3)],
            edges: vec![
                GraphEdge {
                    cable: "_X".to_string(),
                    source: NodeId::circuit("A", 0),
                    sink: NodeId::circuit("D", 0),
                },
                GraphEdge {
                    cable: "_Y".to_string(),
                    source: NodeId::circuit("B", 0),
                    sink: NodeId::circuit("C", 0),
                },
            ],
            ..Default::default()
        };
        let widths = vec![20.0; 4];
        let strict = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        let bary = solve_columns(&graph, &widths, LayoutOrdering::Barycenter);
        for i in 0..4 {
            assert_eq!(
                strict[i].0, bary[i].0,
                "node {i} must keep its column under both orderings"
            );
        }
        assert_columns_do_not_overlap(&graph, &widths, &strict);
        assert_columns_do_not_overlap(&graph, &widths, &bary);
    }

    #[test]
    fn barycenter_ordering_flips_the_crossing_layer() {
        // The crossed A→D / B→C fixture crosses under file order; the
        // barycenter sweeps re-order layer 1 to [D, C], so the within-column
        // vertical order differs from strict while columns stay identical.
        let graph = Graph {
            nodes: vec![node("A", 0), node("B", 1), node("C", 2), node("D", 3)],
            edges: vec![
                GraphEdge {
                    cable: "_X".to_string(),
                    source: NodeId::circuit("A", 0),
                    sink: NodeId::circuit("D", 0),
                },
                GraphEdge {
                    cable: "_Y".to_string(),
                    source: NodeId::circuit("B", 0),
                    sink: NodeId::circuit("C", 0),
                },
            ],
            ..Default::default()
        };
        let widths = vec![20.0; 4];
        let strict = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        let bary = solve_columns(&graph, &widths, LayoutOrdering::Barycenter);
        // Strict keeps file order: C (index 2) above D (index 3) in column 1.
        assert!(
            strict[2].1 < strict[3].1,
            "strict keeps file order in column 1"
        );
        // Barycenter flips layer 1 to [D, C]: D now stacks above C.
        assert!(
            bary[3].1 < bary[2].1,
            "barycenter re-orders column 1 to [D, C]"
        );
        // Columns unchanged: A/B still left of C/D.
        assert!(strict[0].0 < strict[2].0);
        assert_eq!(strict[0].0, bary[0].0);
        assert_eq!(strict[3].0, bary[3].0);
    }

    #[test]
    fn barycenter_ordering_is_deterministic() {
        let graph = make_graph(
            20,
            &[(0, 2), (1, 2), (2, 3), (3, 4), (4, 0), (5, 6), (6, 7)],
        );
        let widths: Vec<f32> = (0..20).map(|i| 20.0 + (i % 4) as f32 * 3.0).collect();
        let a = solve_columns(&graph, &widths, LayoutOrdering::Barycenter);
        let b = solve_columns(&graph, &widths, LayoutOrdering::Barycenter);
        assert_eq!(a, b);
        assert_finite_and_bounded(&a);
    }

    // ── graph-column-layout: node-size estimator (task 1.3) ─────────────────

    #[test]
    fn estimated_widths_scale_with_title_length_and_ports() {
        // The estimator mirrors the renderer's inputs: title characters (with
        // the repeated-instance suffix) and one marker width per port
        // direction the node has edges for.
        let graph = Graph {
            nodes: vec![
                node("osc", 0),
                node("mixer", 1),
                node("clocktool", 2),
                node("copy", 3),
                instance_node("copy", 1, 4),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_CLK".to_string(),
                    source: NodeId::circuit("clocktool", 0),
                    sink: NodeId::circuit("osc", 0),
                },
                GraphEdge {
                    cable: "_OUT".to_string(),
                    source: NodeId::circuit("osc", 0),
                    sink: NodeId::circuit("mixer", 0),
                },
            ],
            ..Default::default()
        };
        let widths = estimated_widths(&graph);
        // Longer titles estimate wider: clocktool > mixer > osc.
        assert!(
            widths[2] > widths[1],
            "clocktool should estimate wider than mixer"
        );
        assert!(
            widths[1] > widths[0],
            "mixer should estimate wider than osc"
        );
        // Repeated instances carry the `(1)` suffix, so the second copy is
        // wider than the first (plain) one.
        assert!(
            widths[4] > widths[3],
            "copy (1) should estimate wider than copy"
        );
        // Ports add marker width: osc has an input and an output; a port-less
        // node is narrower than an equally titled one with ports.
        let no_ports = estimated_widths(&Graph {
            nodes: vec![node("osc", 0)],
            edges: vec![],
            ..Default::default()
        });
        assert!(
            widths[0] > no_ports[0],
            "osc with ports should exceed the port-less osc"
        );
    }

    #[test]
    fn no_overlap_holds_with_estimated_widths() {
        // Task 1.3: the no-overlap property holds when the placement feeds on
        // the estimator's widths instead of hand-chosen ones. Mixed title
        // lengths exercise wide and narrow blocks together.
        let graph = Graph {
            nodes: vec![
                node("osc", 0),
                node("mixer", 1),
                node("clocktool", 2),
                node("p8s8", 3),
                node("resonant_filter", 4),
                node("copy", 5),
                node("copy", 6),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_CLK".to_string(),
                    source: NodeId::circuit("clocktool", 0),
                    sink: NodeId::circuit("osc", 0),
                },
                GraphEdge {
                    cable: "_OSC".to_string(),
                    source: NodeId::circuit("osc", 0),
                    sink: NodeId::circuit("mixer", 0),
                },
                GraphEdge {
                    cable: "_P".to_string(),
                    source: NodeId::circuit("p8s8", 0),
                    sink: NodeId::circuit("mixer", 0),
                },
                GraphEdge {
                    cable: "_F".to_string(),
                    source: NodeId::circuit("resonant_filter", 0),
                    sink: NodeId::circuit("mixer", 0),
                },
            ],
            ..Default::default()
        };
        let widths = estimated_widths(&graph);
        let positions = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        assert_columns_do_not_overlap(&graph, &widths, &positions);
    }

    // ── graph-zoom-node-scaling: world-space node sizing (task 1.1) ──────────

    /// Mixed-title graph reused by the world-size tests: wide and narrow blocks
    /// together, plus a repeated-instance pair so the estimator's suffix path
    /// is exercised.
    fn world_size_graph() -> Graph {
        Graph {
            nodes: vec![
                node("osc", 0),
                node("mixer", 1),
                node("clocktool", 2),
                node("p8s8", 3),
                node("resonant_filter", 4),
                node("copy", 5),
                instance_node("copy", 1, 6),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_CLK".to_string(),
                    source: NodeId::circuit("clocktool", 0),
                    sink: NodeId::circuit("osc", 0),
                },
                GraphEdge {
                    cable: "_OSC".to_string(),
                    source: NodeId::circuit("osc", 0),
                    sink: NodeId::circuit("mixer", 0),
                },
                GraphEdge {
                    cable: "_P".to_string(),
                    source: NodeId::circuit("p8s8", 0),
                    sink: NodeId::circuit("mixer", 0),
                },
                GraphEdge {
                    cable: "_F".to_string(),
                    source: NodeId::circuit("resonant_filter", 0),
                    sink: NodeId::circuit("mixer", 0),
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn node_world_sizes_mirror_estimator_widths() {
        let graph = world_size_graph();
        let widths = estimated_widths(&graph);
        let sizes = node_world_sizes(&graph);

        assert_eq!(sizes.len(), graph.nodes.len(), "one size per node");
        for (i, &(w, h)) in sizes.iter().enumerate() {
            assert_eq!(
                w, widths[i],
                "node {i} world width must be the estimator value"
            );
            assert_eq!(
                h, NODE_WORLD_H,
                "node {i} world height must be NODE_WORLD_H"
            );
        }

        // The body height must leave a visible gap on a vertical slot; the
        // invariant itself is asserted at compile time beside `NODE_WORLD_H`.
    }

    #[test]
    fn node_world_sizes_fit_reserved_column_blocks() {
        // The world width must never exceed the horizontal space the column
        // arrangement reserves for the node's column, so a drawn frame stays
        // inside its block and nodes cannot overlap at any zoom
        // (graph-zoom-node-scaling, design decision 1).
        let graph = world_size_graph();
        let widths = estimated_widths(&graph);
        let sizes = node_world_sizes(&graph);
        let positions = solve_columns(&graph, &widths, LayoutOrdering::Strict);

        // The solver's own reserved block width per column: widest member,
        // snapped up to 2·GRID_SNAP so block centers land on the grid.
        let depth = circuit_depth(&graph);
        let (cols, ncols) = assign_columns(&graph, &depth);
        let block_units = 2.0 * GRID_SNAP;
        let mut reserved = vec![0.0f32; ncols];
        for i in 0..graph.nodes.len() {
            reserved[cols[i]] = reserved[cols[i]].max(widths[i]);
        }
        for bw in &mut reserved {
            *bw = (*bw / block_units).ceil() * block_units;
        }

        for (i, &(w, _)) in sizes.iter().enumerate() {
            assert!(
                w <= reserved[cols[i]] + 1e-3,
                "node {i} world width {w} exceeds its column's reserved block {}",
                reserved[cols[i]]
            );
        }

        // Stronger check against the solver's real output: the drawn world rects
        // (centered on the solved positions) are pairwise non-overlapping.
        for a in 0..graph.nodes.len() {
            for b in (a + 1)..graph.nodes.len() {
                let (aw, ah) = sizes[a];
                let (bw, bh) = sizes[b];
                let overlap_x = (positions[a].0 - positions[b].0).abs() < (aw + bw) / 2.0 - 1e-3;
                let overlap_y = (positions[a].1 - positions[b].1).abs() < (ah + bh) / 2.0 - 1e-3;
                assert!(
                    !(overlap_x && overlap_y),
                    "nodes {a} and {b} overlap: positions {:?} / {:?}, sizes {:?} / {:?}",
                    positions[a],
                    positions[b],
                    sizes[a],
                    sizes[b]
                );
            }
        }
    }

    // ── graph-column-layout: fixed outer columns (task 2.3) ─────────────────

    /// Node of the given kind with the matching `NodeId` variant (Controller
    /// type/ordinal, Jack token), so mixed-kind graphs mirror graph.rs.
    fn kind_node(name: &str, kind: NodeKind, section_index: usize) -> GraphNode {
        let id = match kind {
            NodeKind::Circuit => NodeId::circuit(name, 0),
            NodeKind::Controller => NodeId::Controller(name.to_string(), 0),
            NodeKind::InputJack | NodeKind::OutputJack => NodeId::Jack(name.to_string()),
        };
        GraphNode {
            id,
            kind,
            circuit: name.to_string(),
            instance_index: 0,
            section_index,
        }
    }

    /// Synthetic mixed-kind graph over explicit (source, sink) index pairs.
    fn make_mixed_graph(nodes: Vec<GraphNode>, edges: &[(usize, usize)]) -> Graph {
        let edges: Vec<GraphEdge> = edges
            .iter()
            .map(|&(s, t)| GraphEdge {
                cable: "_C".to_string(),
                source: nodes[s].id.clone(),
                sink: nodes[t].id.clone(),
            })
            .collect();
        Graph {
            nodes,
            edges,
            clusters: vec![],
            validation: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn controller_and_input_jacks_sit_left_output_jacks_right() {
        // Task 2.3 / design D4: on the column path, controllers and input
        // jacks share the left outer column, output jacks the right outer
        // column, and circuit columns run between them (circuit-only depth
        // excludes the outer nodes).
        let nodes = vec![
            kind_node("ctrl", NodeKind::Controller, 0),
            kind_node("in", NodeKind::InputJack, 1),
            kind_node("osc", NodeKind::Circuit, 2),
            kind_node("mixer", NodeKind::Circuit, 3),
            kind_node("out", NodeKind::OutputJack, 4),
        ];
        let graph = make_mixed_graph(nodes, &[(0, 2), (1, 2), (2, 3), (3, 4)]);
        let widths = vec![20.0; 5];
        let positions = solve_columns(&graph, &widths, LayoutOrdering::Strict);

        // Controller and input jack share the leftmost column.
        assert_eq!(
            positions[0].0, positions[1].0,
            "controller and input jack must share the left column"
        );
        let left_x = positions[0].0;
        // Circuits sit strictly between the outer columns.
        assert!(
            positions[2].0 > left_x,
            "circuit osc must sit right of the left column"
        );
        assert!(
            positions[3].0 > positions[2].0,
            "circuit mixer must sit right of osc"
        );
        assert!(
            positions[4].0 > positions[3].0,
            "output jack must sit right of the circuits"
        );
        // Grid snap holds on the mixed-kind path.
        for (x, y) in &positions {
            let sx = (x / GRID_SNAP).round() * GRID_SNAP;
            let sy = (y / GRID_SNAP).round() * GRID_SNAP;
            assert!((sx - x).abs() < 1e-3, "x {x} off the {GRID_SNAP} grid");
            assert!((sy - y).abs() < 1e-3, "y {y} off the {GRID_SNAP} grid");
        }
        assert_columns_do_not_overlap(&graph, &widths, &positions);
    }

    #[test]
    fn outer_columns_do_not_shift_circuit_columns() {
        // Design D4: circuit columns come from the circuit-only depth, so
        // outer-column nodes never widen or renumber the circuit block range.
        // The same two-circuit chain yields the same circuit columns whether
        // or not jacks/controllers are present.
        let bare = Graph {
            nodes: vec![node("a", 0), node("b", 1)],
            edges: vec![GraphEdge {
                cable: "_AB".to_string(),
                source: NodeId::circuit("a", 0),
                sink: NodeId::circuit("b", 0),
            }],
            ..Default::default()
        };
        let with_outer = make_mixed_graph(
            vec![
                kind_node("in", NodeKind::InputJack, 0),
                kind_node("a", NodeKind::Circuit, 1),
                kind_node("b", NodeKind::Circuit, 2),
                kind_node("out", NodeKind::OutputJack, 3),
            ],
            &[(0, 1), (1, 2), (2, 3)],
        );
        let widths = vec![20.0; 4];
        let bare_pos = solve_columns(&bare, &[20.0; 2], LayoutOrdering::Strict);
        let outer_pos = solve_columns(&with_outer, &widths, LayoutOrdering::Strict);
        // Circuit pitch identical: the a→b gap (block 20 + spacing 80) is the
        // same with and without the outer columns.
        let bare_pitch = bare_pos[1].0 - bare_pos[0].0;
        let outer_pitch = outer_pos[2].0 - outer_pos[1].0;
        assert_eq!(
            bare_pitch, outer_pitch,
            "outer columns must not change circuit pitch"
        );
    }

    // ── graph-column-layout: task 4.1 audit coverage ───────────────────────

    #[test]
    fn dense_normalization_collapses_gaps_and_duplicate_depths() {
        // Dense normalization contract (design D2): depth values map to dense
        // columns 0..N-1 with no empty columns. Longest-path depth is always
        // contiguous, so the gap collapse is pinned white-box on the helper
        // (depth 5 lands at column 2, not 5), while the duplicate collapse is
        // pinned end-to-end on a fan (all depth-0 sources share one column).
        assert_eq!(dense_columns(&[0, 2, 5, 2, 0, 5]), vec![0, 1, 2, 1, 0, 2]);
        assert_eq!(dense_columns(&[0, 2, 5]), vec![0, 1, 2]);
        assert_eq!(dense_columns(&[3, 3, 3]), vec![0, 0, 0]);

        // End-to-end: a fan of sources into one sink — every source sits at
        // depth 0 and must collapse into a single column on distinct slots.
        let graph = make_graph(5, &[(0, 4), (1, 4), (2, 4), (3, 4)]);
        let widths = vec![20.0; 5];
        let positions = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        for i in 1..4 {
            assert_eq!(
                positions[i].0, positions[0].0,
                "source {i} must share the depth-0 column"
            );
        }
        for a in 0..4 {
            for b in (a + 1)..4 {
                assert!(
                    (positions[a].1 - positions[b].1).abs() >= VERTICAL_SPACING,
                    "sources {a} and {b} share vertical space"
                );
            }
        }
        assert!(
            positions[4].0 > positions[0].0,
            "the sink must sit in its own column right of the sources"
        );
    }

    #[test]
    fn barycenter_columns_snap_to_grid_and_never_overlap() {
        // No-overlap and grid-snap hold under the barycenter ordering too: a
        // 3×2 crossed layer gives multi-member columns that barycenter
        // re-orders vertically, and the stacking contract must still hold
        // with fractional widths (the strict-only snap/no-overlap tests do
        // not cover this ordering on a non-trivial column).
        let graph = Graph {
            nodes: vec![
                node("A", 0),
                node("B", 1),
                node("C", 2),
                node("D", 3),
                node("E", 4),
            ],
            edges: vec![
                GraphEdge {
                    cable: "_AE".to_string(),
                    source: NodeId::circuit("A", 0),
                    sink: NodeId::circuit("E", 0),
                },
                GraphEdge {
                    cable: "_BD".to_string(),
                    source: NodeId::circuit("B", 0),
                    sink: NodeId::circuit("D", 0),
                },
                GraphEdge {
                    cable: "_BE".to_string(),
                    source: NodeId::circuit("B", 0),
                    sink: NodeId::circuit("E", 0),
                },
                GraphEdge {
                    cable: "_CD".to_string(),
                    source: NodeId::circuit("C", 0),
                    sink: NodeId::circuit("D", 0),
                },
            ],
            ..Default::default()
        };
        let widths: Vec<f32> = (0..5).map(|i| 17.5 + i as f32 * 2.1).collect();
        let positions = solve_columns(&graph, &widths, LayoutOrdering::Barycenter);
        for (x, y) in &positions {
            let sx = (x / GRID_SNAP).round() * GRID_SNAP;
            let sy = (y / GRID_SNAP).round() * GRID_SNAP;
            assert!((sx - x).abs() < 1e-3, "x {x} off the {GRID_SNAP} grid");
            assert!((sy - y).abs() < 1e-3, "y {y} off the {GRID_SNAP} grid");
        }
        assert_columns_do_not_overlap(&graph, &widths, &positions);
    }

    #[test]
    fn cyclic_patch_columns_terminate_and_stay_dense() {
        // Capped Bellman-Ford on a cycle still terminates (design D2): depth
        // keeps growing but stays within a contiguous band, so the dense
        // normalization yields a finite 0..N-1 column range with no empty
        // columns. Here the 4-cycle plus three depth-0 fans map to exactly 5
        // columns under both orderings, with the no-overlap, grid-snap, and
        // determinism contracts intact.
        let graph = make_graph(
            7,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0), // 4-cycle
                (4, 1),
                (5, 1),
                (6, 2), // fans into the cycle
            ],
        );
        let widths = vec![20.0; 7];
        for ordering in [LayoutOrdering::Strict, LayoutOrdering::Barycenter] {
            let positions = solve_columns(&graph, &widths, ordering);
            assert_finite_and_bounded(&positions);
            let (cols, ncols) = assign_columns(&graph, &circuit_depth(&graph));
            assert_eq!(
                ncols, 5,
                "cycle + fans must dense-normalize to 5 columns, got {ncols}"
            );
            assert_eq!(cols.iter().copied().max(), Some(ncols - 1));
            for c in 0..ncols {
                assert!(cols.contains(&c), "column {c} is empty");
            }
            assert_columns_do_not_overlap(&graph, &widths, &positions);
            for (x, y) in &positions {
                let sx = (x / GRID_SNAP).round() * GRID_SNAP;
                let sy = (y / GRID_SNAP).round() * GRID_SNAP;
                assert!((sx - x).abs() < 1e-3, "x {x} off the {GRID_SNAP} grid");
                assert!((sy - y).abs() < 1e-3, "y {y} off the {GRID_SNAP} grid");
            }
            assert_eq!(
                solve_columns(&graph, &widths, ordering),
                positions,
                "cyclic solve under {ordering:?} must be deterministic"
            );
        }
    }

    // ── graph-column-layout: pins as fixed-position anchors (task 3.2) ─────

    #[test]
    fn pinned_column_node_keeps_position_while_rest_arrange_around_it() {
        // Design D6: pins are fixed-position anchors on the column path. Pin
        // node 0 (column 0, shared with node 1) at a grid-aligned position one
        // slot lower than its natural slot: it must sit exactly there while
        // node 1 stacks around it without overlap.
        let graph = make_graph(6, &[(0, 2), (1, 2), (2, 3), (3, 4), (3, 5)]);
        let widths: Vec<f32> = (0..6).map(|i| 15.0 + (i % 7) as f32 * 6.3).collect();
        let natural = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        let anchor = (natural[0].0, natural[0].1 + VERTICAL_SPACING);
        let positions =
            solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[(0, anchor)]);
        assert_eq!(
            positions[0], anchor,
            "pinned node must sit exactly at its anchor"
        );
        // The rest arranged around it: node 1 moved off the pinned slot.
        assert!(
            (positions[1].1 - positions[0].1).abs() >= VERTICAL_SPACING,
            "node 1 overlaps the pinned slot"
        );
        // Unpinned nodes still grid-snap.
        for (x, y) in &positions {
            let sx = (x / GRID_SNAP).round() * GRID_SNAP;
            let sy = (y / GRID_SNAP).round() * GRID_SNAP;
            assert!((sx - x).abs() < 1e-3, "x {x} off the {GRID_SNAP} grid");
            assert!((sy - y).abs() < 1e-3, "y {y} off the {GRID_SNAP} grid");
        }
        assert_columns_do_not_overlap(&graph, &widths, &positions);
        // Determinism per (graph, widths, ordering, pins).
        let again = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[(0, anchor)]);
        assert_eq!(positions, again);
        // The no-pin entry delegates with an empty pin set.
        assert_eq!(
            solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[]),
            natural
        );
    }

    #[test]
    fn pinned_column_node_keeps_anchor_against_its_natural_column_slot() {
        // The anchor wins over the natural column layout: pin node 2 (column
        // 1) at a position the column path would never give it — the slot of
        // the neighbouring column's first node. It must stay exactly there
        // and no other node may collide with it.
        let graph = make_graph(6, &[(0, 2), (1, 2), (2, 3), (3, 4), (3, 5)]);
        let widths: Vec<f32> = (0..6).map(|i| 20.0 + (i % 3) as f32 * 5.0).collect();
        let natural = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        // Column 1's block center, but one slot lower than its natural y.
        let anchor = (natural[2].0, natural[2].1 + VERTICAL_SPACING);
        let positions =
            solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[(2, anchor)]);
        assert_eq!(positions[2], anchor);
        for (i, (x, y)) in positions.iter().enumerate() {
            let sx = (x / GRID_SNAP).round() * GRID_SNAP;
            let sy = (y / GRID_SNAP).round() * GRID_SNAP;
            assert!((sx - x).abs() < 1e-3, "node {i} x {x} off the grid");
            assert!((sy - y).abs() < 1e-3, "node {i} y {y} off the grid");
        }
        assert_columns_do_not_overlap(&graph, &widths, &positions);
    }

    // ── sections-black-hole-solver: lane slots + outer lanes (tasks 6.3, 3.8) ──

    #[test]
    fn sections_mode_slice_spring_holds_non_clustered_circuits_at_their_lane_slot() {
        // Lane-slot contract (task 3.8, sharpened by 3.9): with no banner groups
        // there are no section-hole anchors, so each circuit is held by the slice
        // spring alone — and that spring pulls toward the node's *own seed target*
        // in both axes, not `slice_index * HORIZONTAL_SPACING`. `make_graph`
        // assigns section_index == index, so the four circuits own lanes 0..3;
        // every title is `n{i}` (2 chars), so every lane is `estimated_widths[i]`
        // = 32 wide and the packer lays the slots out at
        // x = i * (32 + SECTIONS_LANE_GAP) = 0, 72, 144, 216, all in row 0 (y = 0).
        // Every lane here holds exactly one node, so each node's seed target *is*
        // its lane slot and the expected targets below are the 3.8 values, derived
        // from that arithmetic rather than from the solver's `sections_lanes`.
        let graph = make_graph(4, &[]);
        assert!(graph.clusters.is_empty(), "fixture must have no clusters");
        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);

        let lane_w = estimated_widths(&graph)[0].min(SECTIONS_LANE_MAX_NODE_W);
        assert_eq!(lane_w, 32.0, "the 2-char fixture lanes are 32 wide");

        // Tolerance: SECTIONS_LANE_GAP / 4 = 10 world units. The slice spring is
        // deliberately weak (SLICE_K = 0.1) and the nodes are collinear in row 0,
        // so size-aware repulsion (required 80 between two 80-tall bodies) fights
        // the spring head-on along x; the worst observed deviation is 4.91. With
        // the slice spring targeting the old `slice_index * HORIZONTAL_SPACING`
        // axis instead, the targets would be 0/80/160/240 and node 1 would read
        // 70.84 — 9.16 off, while all four targets drift with the lane width.
        let tolerance = SECTIONS_LANE_GAP / 4.0;
        for (i, &(x, y)) in positions.iter().enumerate() {
            let target_x = i as f32 * (lane_w + SECTIONS_LANE_GAP);
            assert!(
                (x - target_x).abs() <= tolerance,
                "circuit {i} x={x} is not within {tolerance} of its lane slot {target_x}"
            );
            // Row 0: the single-node lanes all sit on the row's top edge.
            assert!(
                y.abs() <= tolerance,
                "circuit {i} y={y} left its lane's row (slot_y = 0) by more than {tolerance}"
            );
        }
        // Lanes stay ordered left-to-right inside the row (no lane crossing).
        for pair in positions.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "lane x must increase with patch order"
            );
        }
    }

    #[test]
    fn sections_mode_outer_nodes_stay_at_the_outer_lanes() {
        // `slice_index`'s contract (D1) puts controllers and input jacks in the
        // left outer lane (0) and output jacks in the right outer lane
        // (max_circuit_lane + 1). The index-level test pins the values; this pins
        // that the solve lands the nodes on those lanes' *slots*.
        //
        // Hand-derived lane geometry (task 3.8 caps each lane at its widest
        // member's `min(estimated_width, SECTIONS_LANE_MAX_NODE_W)`, all widths
        // here are title chars × 8 + 16 padding):
        //   lane 0 = c0 (32), ctrl (48), IN (32)  -> width 48 -> slot x 0
        //   lane 1 = c1 (32)                      -> slot x 0 + 48 + 40  = 88
        //   lane 2 = c2 (32)                      -> slot x 88 + 32 + 40 = 160
        //   lane 3 = OUT (32)                     -> slot x 160 + 32 + 40 = 232
        // All lanes hold one row, so every slot y is 0.
        let nodes = vec![
            node("c0", 0),
            node("c1", 1),
            node("c2", 2),
            GraphNode {
                id: NodeId::Controller("ctrl".to_string(), 0),
                kind: NodeKind::Controller,
                circuit: "ctrl".to_string(),
                instance_index: 0,
                section_index: 0,
            },
            GraphNode {
                id: NodeId::Jack("IN".to_string()),
                kind: NodeKind::InputJack,
                circuit: "IN".to_string(),
                instance_index: 0,
                section_index: 0,
            },
            GraphNode {
                id: NodeId::Jack("OUT".to_string()),
                kind: NodeKind::OutputJack,
                circuit: "OUT".to_string(),
                instance_index: 0,
                section_index: 0,
            },
        ];
        let graph = Graph {
            nodes,
            ..Default::default()
        };
        assert_eq!(slice_index(&graph), vec![0, 1, 2, 0, 0, 3]);
        assert_eq!(
            sections_lanes(&graph).slot,
            vec![(0.0, 0.0), (88.0, 0.0), (160.0, 0.0), (232.0, 0.0)],
            "lane slots must track the hand-derived width-aware geometry"
        );

        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);

        // Tolerance: SECTIONS_LANE_GAP / 4 = 10 world units (worst observed
        // deviation 5.0, on the output jack: it is the outermost node, so
        // repulsion from lane 2 has nothing to push back against).
        let tolerance = SECTIONS_LANE_GAP / 4.0;
        for i in [0, 3, 4] {
            assert!(
                positions[i].0.abs() <= tolerance,
                "node {i} x={} must sit at the left outer lane slot (0)",
                positions[i].0
            );
        }
        let output_target = 232.0;
        assert!(
            (positions[5].0 - output_target).abs() <= tolerance,
            "output jack x={} must sit at the right outer lane slot ({output_target})",
            positions[5].0
        );
        // The outer lanes bracket the circuit lanes.
        let circuit_max_x = positions[..3]
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            positions[5].0 > circuit_max_x,
            "output jack must sit right of the circuits"
        );
    }

    // ── sections-black-hole-solver: size-aware repulsion (D3, task 6.3) ─────

    #[test]
    fn sections_mode_size_aware_repulsion_acts_beyond_the_fixed_radius() {
        // Unchanged by the lane contract (task 3.8): this pins the *force* the
        // sections path applies, and the wrapped lane grid relies on it exactly
        // as before — lane sizing decides the pitch, this force decides whether a
        // pair of bodies that ends up closer than `required` is pushed apart.
        //
        // D3: sections-mode repulsion is size-aware and cut off by the pair's
        // *required separation* `req = 0.5 * (extent_i + extent_j)` (extent =
        // max(width, height)), not by the fixed REPULSION_RADIUS. A 200-wide node
        // next to an equal one needs a center distance of 200 to clear it, so
        // repulsion must reach well past the fixed radius (120). This pins the
        // force contract the solver relies on (nothing else exercises this
        // function directly).
        let a = (0.0, 0.0);
        let b = (150.0, 0.0); // REPULSION_RADIUS (120) < 150 < req (200)
        assert_eq!(
            repulsion_force(a, b),
            (0.0, 0.0),
            "fixed-radius repulsion must cut off past REPULSION_RADIUS"
        );
        let required = 200.0;
        let f = repulsion_force_size_aware(a, b, required, REPULSION_GAIN_DEFAULT);
        assert!(
            f.0 < 0.0 && f.1 == 0.0,
            "size-aware repulsion must push apart at d=150 with required {required}, got {f:?}"
        );

        // Zero at and beyond the required separation: no phantom push at rest.
        assert_eq!(
            repulsion_force_size_aware(
                (0.0, 0.0),
                (required, 0.0),
                required,
                REPULSION_GAIN_DEFAULT
            ),
            (0.0, 0.0),
            "repulsion must be exactly zero at d == required"
        );
        assert_eq!(
            repulsion_force_size_aware(
                (0.0, 0.0),
                (required + 50.0, 0.0),
                required,
                REPULSION_GAIN_DEFAULT
            ),
            (0.0, 0.0),
            "repulsion must be zero beyond required"
        );

        // Monotonic in penetration: a deeper overlap pushes at least as hard,
        // and the force is still positive just inside the boundary.
        let mag = |d: f32| {
            repulsion_force_size_aware((0.0, 0.0), (d, 0.0), required, REPULSION_GAIN_DEFAULT)
                .0
                .abs()
        };
        assert!(mag(1.0) >= mag(50.0), "force must not weaken as d falls");
        assert!(mag(50.0) >= mag(150.0), "force must not weaken as d falls");
        assert!(mag(150.0) >= mag(199.9), "force must not weaken as d falls");
        assert!(mag(199.9) > 0.0, "force must stay positive inside required");

        // Finite at the coincident point: no division by zero, fixed nudge.
        let coincident =
            repulsion_force_size_aware((0.0, 0.0), (0.0, 0.0), required, REPULSION_GAIN_DEFAULT);
        assert!(
            coincident.0.is_finite() && coincident.1.is_finite(),
            "coincident nodes must get a finite nudge, got {coincident:?}"
        );
    }

    #[test]
    fn sections_mode_keeps_large_nodes_beyond_their_size_required_distance() {
        // D3 end-to-end at the widths the design has to survive. Both cases are
        // two equal-width circuits with no edges, at lanes 0 and 2, so their
        // slice springs pull back toward those lanes' slots and are the restoring
        // force size-aware repulsion must beat. Under the lane contract (task
        // 3.8) the slot pitch is width-aware: lane 0 is `min(w, cap)` wide, lane 1
        // is empty (`SECTIONS_LANE_MIN_W` = 40), so slot 2 sits at
        // `lane_w + 2 · SECTIONS_LANE_GAP` — 296 for the 176-wide case and 360
        // for the 496-wide case (whose lane is capped at
        // `SECTIONS_LANE_MAX_NODE_W` = 240). `required` is the combined
        // half-extent `0.5 * (extent_a + extent_b)` with `extent = max(w, h)`:
        // below that center distance the bodies overlap.
        //
        //   20-char titles -> 176 wide, required 176, slot pitch 296 (clear)
        //   60-char titles -> 496 wide, required 496, slot pitch 360 (216 short,
        //                   so repulsion must win 216 units of separation back
        //                   from the two slice springs)
        //
        // Tolerance: 10% of `required` (49.6 units for the wide case). The force
        // is zero *at* `required`, so with FRICTION damping the pair coasts a
        // little past it rather than landing exactly on it; the tolerance absorbs
        // that without allowing overlap. Observed exactly 296.0 for the 176-wide
        // case (its lane pitch already clears `required` = 176, so both nodes sit
        // on their slots and repulsion never fires) and 496.49 for the 496-wide
        // case — the pair clears `required` because repulsion wins 216 units of
        // separation back from the two slice springs.
        for (chars, expected_required) in [(20usize, 176.0f32), (60, 496.0)] {
            let graph = Graph {
                nodes: vec![node(&"a".repeat(chars), 0), node(&"b".repeat(chars), 2)],
                ..Default::default()
            };
            let sizes = node_world_sizes(&graph);
            let required = (sizes[0].0.max(sizes[0].1) + sizes[1].0.max(sizes[1].1)) / 2.0;
            assert_eq!(
                required, expected_required,
                "{chars}-char fixture must require {expected_required}"
            );

            let sections = solve_sections(&graph, &[], SolveKnobs::default());
            let force = solve(&graph, &[], SolveKnobs::default());
            assert_finite_and_bounded(&sections);
            assert_finite_and_bounded(&force);

            let sections_gap = dist(sections[0], sections[1]);
            let force_gap = dist(force[0], force[1]);
            let tolerance = required * 0.1;
            assert!(
                sections_gap >= required - tolerance,
                "sections gap {sections_gap} must keep {chars}-char bodies clear \
                 (required {required}, tolerance {tolerance})"
            );
            // Re-blessed (design D8): force mode is size-aware now too, so it
            // also keeps the pair clear. HEAD's point-repulsion left both widths
            // at the seed gap (measured 120.10, i.e. 56-376 units of overlap);
            // the size-aware solver measures 178.97 (20-char) and 506.39
            // (60-char). The old expectation asserted the overlap this change
            // removes, and the old "sections separates more than force" claim no
            // longer holds for the wide case (496.49 vs 506.39): sections mode is
            // tethered to its lane slots while untethered force mode coasts a
            // little past `required`.
            assert!(
                force_gap >= required - tolerance,
                "force mode gap {force_gap} must keep {chars}-char bodies clear \
                 (required {required}, tolerance {tolerance})"
            );
        }
    }

    #[test]
    fn force_mode_positions_are_deterministic_and_non_overlapping() {
        // Task 3.4 / design D8. This started as
        // `force_mode_positions_are_unchanged_by_size_aware_repulsion` (a
        // sections-change guard asserting force mode was byte-identical), was
        // re-blessed once into a perturbation vector, and is replaced here by the
        // contract the force path actually promises. Byte-identity is the wrong
        // guard: size-aware repulsion exists precisely to move positions where
        // bodies intersect, so a re-blessed vector only re-records whatever the
        // solver happens to do. The pair that replaces it is stronger — the knob
        // tuple is the determinism key (same graph + same knobs → identical
        // positions) and the output is non-overlapping, both asserted on a graph
        // that exercises every force-mode mechanism: cable springs, grid-hashed
        // repulsion, and cohesion (the banner cluster makes `cohesion` fire).
        let mut graph = make_graph(5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        graph.nodes[0].circuit = "a".repeat(2);
        graph.nodes[1].circuit = "b".repeat(8);
        graph.nodes[2].circuit = "c".to_string();
        graph.nodes[3].circuit = "d".repeat(4);
        graph.nodes[4].circuit = "e".repeat(11);
        graph.clusters = vec![Cluster {
            title: "g".to_string(),
            section_range: 1..3,
        }];
        let sizes = node_world_sizes(&graph);
        let first = solve(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&first);
        assert_eq!(
            first,
            solve(&graph, &[], SolveKnobs::default()),
            "same graph + same knob tuple must reproduce the force-mode positions"
        );
        assert_pairs_clear(&sizes, &first, 0.0, "force-mode solve");

        // `local_resettle` (the drag path) enters the same core from a moved
        // seed, so it carries the same two guarantees. The radius covers this
        // whole graph and nothing is pinned, so every body is movable and the
        // non-overlap assertion is the whole-set one, not a neighbourhood.
        let dragged = |shift: (f32, f32)| {
            let mut positions = seed_positions(&graph);
            positions[3].0 += shift.0;
            positions[3].1 += shift.1;
            let moved = graph.nodes[3].id.clone();
            assert!(local_resettle(
                &graph,
                &mut positions,
                &moved,
                LOCAL_RADIUS,
                LOCAL_ITERATIONS,
                &[],
                SolveKnobs::default(),
            ));
            positions
        };
        let settled = dragged((25.0, 40.0));
        assert_finite_and_bounded(&settled);
        assert_eq!(
            settled,
            dragged((25.0, 40.0)),
            "same drag + same knob tuple must reproduce the re-settled positions"
        );
        assert_pairs_clear(&sizes, &settled, 0.0, "force-mode local re-settle");
    }

    #[test]
    fn sections_mode_empty_graph_and_single_node_do_not_panic() {
        // Empty graph: `run_iterations_with_forces` early-returns on zero nodes,
        // so sections mode must yield no positions rather than panicking.
        let empty = Graph::default();
        assert!(
            solve_sections(&empty, &[], SolveKnobs::default()).is_empty(),
            "empty graph must produce no positions"
        );
        // Single node: exactly one finite, bounded position.
        let single = make_graph(1, &[]);
        let positions = solve_sections(&single, &[], SolveKnobs::default());
        assert_eq!(positions.len(), 1);
        assert_finite_and_bounded(&positions);
    }

    /// Bounding box `(width, height)` of a position set.
    fn positions_bbox(positions: &[(f32, f32)]) -> (f32, f32) {
        let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
        for (x, y) in positions {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
        (max_x - min_x, max_y - min_y)
    }

    /// Number of distinct topological-depth layers — the range of the dense
    /// layer ranks the layered seed contracts the depth onto.
    fn distinct_layer_count(graph: &Graph) -> usize {
        dense_columns(&topological_depth(graph))
            .into_iter()
            .max()
            .map_or(1, |last| last + 1)
    }

    #[test]
    fn dense_normalization_preserves_barycenter_layers_and_within_layer_orders() {
        // 1.1 contract (design D1): dense-normalizing the depth must not change
        // the crossing-minimization input. `crossing_minimized_layers` groups
        // nodes by depth *equality* (a `HashMap` keyed by the value), fixes the
        // layer sequence by first appearance in node order, and sorts within a
        // layer by neighbor barycenters — every one of those steps is
        // equality-based, so an injective relabeling of the depth values yields
        // identical layers and identical within-layer orders. Verified here
        // rather than assumed: on a cyclic graph whose raw depths are inflated
        // past `n`, the per-node orders computed from the raw depth and from
        // the dense ranks must match exactly.
        let graph = make_graph(
            8,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (0, 4),
                (4, 5),
                (5, 6),
                (6, 4),
                (2, 7),
                (7, 5),
            ],
        );
        let raw = topological_depth(&graph);
        let dense = dense_columns(&raw);
        let raw_max = raw.iter().copied().max().unwrap_or(0);
        assert!(
            raw_max >= graph.nodes.len(),
            "the fixture must be cycle-inflated (raw depth ≥ n), got max {raw_max}"
        );
        assert!(
            !raw.iter().copied().eq(0..graph.nodes.len()),
            "the fixture's raw depths must be sparse, not contiguous"
        );
        let mut distinct = raw.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            dense.iter().copied().max(),
            Some(distinct.len() - 1),
            "dense ranks must be contiguous 0..N-1"
        );
        assert_eq!(
            crossing_minimized_orders(&graph, &raw),
            crossing_minimized_orders(&graph, &dense),
            "dense normalization changed the barycenter layers or within-layer orders"
        );
        // Directly on the layer structure too: the same layer sequence (same
        // membership, same order) for the raw depths and the dense ranks.
        let index = node_index(&graph);
        let edges = edge_pairs(&graph, &index);
        let active = vec![true; graph.nodes.len()];
        assert_eq!(
            crossing_minimized_layers(graph.nodes.len(), &edges, &raw, &active),
            crossing_minimized_layers(graph.nodes.len(), &edges, &dense, &active),
            "dense normalization changed the layer partition or the layer sequence"
        );
    }

    #[test]
    fn force_mode_seed_and_solve_stay_proportional_on_cyclic_scale_fixture() {
        // 1.1 regression: `fixtures/own_scale.ini` (511 nodes) is cyclic, so the
        // raw Bellman-Ford depth is not bounded by `n` — measured max depth
        // 245,281 (over only 5 distinct values), so the raw-depth seed spans
        // 245,281 × HORIZONTAL_SPACING = 19,622,480 units. The integrator can
        // travel at most `MAX_DISPLACEMENT × SOLVE_ITERATIONS` = 1200 units per
        // node, so a raw-depth seed is unreachable: the measured force-mode bbox
        // width stays at 19,621,300 (proposal / design D1).
        //
        // The dense-normalized seed instead puts exactly one spacing unit
        // between adjacent *distinct layers*, bounding the seed's horizontal
        // extent by `(distinct layers − 1) × HORIZONTAL_SPACING`. The solved
        // width can add at most two integrator travel budgets on top (one node
        // drifting each way from its seed), so it too stays proportional to
        // patch size instead of exploding with cycle length.
        let graph = graph_from_fixture("fixtures/own_scale.ini");
        let n = graph.nodes.len();
        assert!(n > 400, "scale fixture should be large, got {n} nodes");
        let layers = distinct_layer_count(&graph);
        assert!(layers > 1, "scale fixture must have several layers");

        let seed = seed_positions(&graph);
        for (x, y) in &seed {
            assert!(x.is_finite() && y.is_finite(), "non-finite seed ({x}, {y})");
        }
        let (seed_width, _) = positions_bbox(&seed);
        let seed_bound = (layers - 1) as f32 * HORIZONTAL_SPACING * 1.5;
        assert!(
            seed_width <= seed_bound,
            "force seed width {seed_width} exceeds {seed_bound} \
             ((distinct layers − 1) × HORIZONTAL_SPACING × 1.5; {layers} layers); \
             the raw-depth seed measures 19,622,480 here"
        );

        let positions = solve(&graph, &[], SolveKnobs::default());
        assert_eq!(positions.len(), n);
        for (x, y) in &positions {
            assert!(
                x.is_finite() && y.is_finite(),
                "non-finite position ({x}, {y})"
            );
        }
        let travel_budget = MAX_DISPLACEMENT * SOLVE_ITERATIONS as f32;
        let (width, _) = positions_bbox(&positions);
        let solved_bound = (layers - 1) as f32 * HORIZONTAL_SPACING + 2.0 * travel_budget;
        assert!(
            width <= solved_bound,
            "force bbox width {width} exceeds {solved_bound} (the dense seed's \
             {seed_width} extent plus two integrator travel budgets of \
             {travel_budget}); the raw-depth seed smears to ~19,621,300 here \
             (proposal; its 19.6 M extent is unreachable for the integrator)"
        );
    }

    #[test]
    fn sections_mode_scale_is_proportional_to_patch_size_not_cycle_length() {
        // Regression for the exploding sections seed on a large cyclic patch —
        // now asserting the *wrapped-lane* goal (task 3.8) instead of one
        // unbounded ribbon. `fixtures/own_scale.ini` has 511 nodes, feedback
        // cables making the graph cyclic, and 510 lanes (509 patch-order circuit
        // lanes plus the output-jack lane).
        //
        // Before the lane rework the seed was `x = slice_index *
        // HORIZONTAL_SPACING`, `y = global_rank * VERTICAL_SPACING`, which
        // measured a 39,540 × 60,020 ribbon on this fixture (aspect ~40:1). The
        // wrapped seed instead packs lanes of width
        // `min(estimated_width, SECTIONS_LANE_MAX_NODE_W)` into rows of at most
        // `SECTIONS_ROW_TARGET_W`, so the same patch must read roughly square.
        //
        // Measured after the rework: lanes 510, lanes/row 32 (the fullest row),
        // so `ceil(510 / 32) + 1 = 17` rows are budgeted; solved bbox
        // 3761.50 × 2740.69 → aspect 1.37 (3.9 re-measured 3812.00 × 2849.51 →
        // aspect 1.34 with the tether and the stack-aware row pitch).
        //
        // Falsified: restoring the old `slice_index * HORIZONTAL_SPACING` /
        // `global_rank * VERTICAL_SPACING` seed fails the aspect assertion and the
        // per-node drift assertion below.
        let graph = graph_from_fixture("fixtures/own_scale.ini");
        let n = graph.nodes.len();
        assert!(n > 400, "scale fixture should be large, got {n} nodes");

        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_eq!(positions.len(), n);
        for (x, y) in &positions {
            assert!(
                x.is_finite() && y.is_finite(),
                "non-finite position ({x}, {y})"
            );
        }

        // The wrapped seed itself is overlap-free — that is what makes the bounded
        // springs safe to trust. Measured worst seed penetration −28.00 on this
        // fixture (positive = overlapping). Falsified by the 3.9 row-height fix:
        // sizing a row from NODE_WORLD_H alone instead of the lane's *full stack*
        // lets a stacked lane overflow into the next row and puts this at +40.00.
        let seed = sections_seed(&graph);
        let sizes = node_world_sizes(&graph);
        let seed_penetration = worst_penetration(&sizes, &seed.targets);
        assert!(
            seed_penetration <= 0.0,
            "the wrapped seed must be overlap-free, worst seed penetration {seed_penetration}"
        );

        // Lane geometry, from the helper the solver itself uses.
        let lanes = sections_lanes(&graph);
        let lane_count = lanes.slot.len();
        assert!(
            lane_count > 400,
            "the fixture should have hundreds of lanes, got {lane_count}"
        );
        let mut row_ys: Vec<f32> = lanes.slot.iter().map(|s| s.1).collect();
        row_ys.sort_by(|a, b| a.partial_cmp(b).expect("finite slot y"));
        row_ys.dedup();
        let rows = row_ys.len();
        let lanes_per_row = row_ys
            .iter()
            .map(|&y| lanes.slot.iter().filter(|s| s.1 == y).count())
            .max()
            .expect("the fixture has at least one row");
        let row_budget = lane_count.div_ceil(lanes_per_row);
        assert!(
            rows >= 2,
            "a {lane_count}-lane patch must wrap into rows, got {rows}"
        );

        // Overview goal: roughly square (aspect ≤ 4:1) and inside the wrapped
        // grid's own bounds.
        let (width, height) = positions_bbox(&positions);
        let aspect = width.max(height) / width.min(height).max(f32::MIN_POSITIVE);
        assert!(
            aspect <= 4.0,
            "sections bbox {width} x {height} has aspect {aspect}, above the 4:1 \
             overview goal (the old ribbon seed measured ~40:1)"
        );
        let width_bound = SECTIONS_ROW_TARGET_W * 1.5;
        assert!(
            width <= width_bound,
            "sections width {width} exceeds {width_bound} \
             (SECTIONS_ROW_TARGET_W × 1.5)"
        );
        let height_bound = (row_budget + 1) as f32 * (NODE_WORLD_H + SECTIONS_ROW_GAP) * 1.5;
        assert!(
            height <= height_bound,
            "sections height {height} exceeds {height_bound} \
             ((ceil(lanes / lanes_per_row) + 1) × (NODE_WORLD_H + SECTIONS_ROW_GAP) × 1.5)"
        );

        // Lane order is patch order (task 3.9 guards it on the seed, where it is
        // exact): inside every row the lanes ascend left→right, and the next row
        // sits below the previous one.
        for pair in lanes.slot.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if b.1 == a.1 {
                assert!(
                    b.0 > a.0,
                    "lanes inside one row must ascend left→right: {a:?} then {b:?}"
                );
            } else {
                assert!(b.1 > a.1, "a wrapped row must sit below the previous one");
            }
        }

        // Every node stays inside the space its own lane reserved. The springs are
        // bounded by SECTIONS_TETHER (60 = 1.5 lane gaps) and the overlap-removal
        // pass may add at most one more lane gap while separating a pair, so the
        // tolerance is 60 + 40 = 100 world units. Measured worst drift is 73.75
        // (y, from a separation push) / 60.00 (x — the tether at its limit). Before
        // 3.9 the same drift was 1180.0, the integrator's whole travel budget
        // (`MAX_DISPLACEMENT × (SOLVE_ITERATIONS − 1)`), reached by nodes dragged at
        // full speed toward a far-away anchor: that is what let nodes leave the lane
        // grid and land on each other. A node can no longer do either, which is what
        // pins the layout to patch-size proportion.
        let drift_tol = SECTIONS_TETHER + SECTIONS_LANE_GAP;
        let mut worst_drift = 0.0f32;
        for (i, &(x, y)) in positions.iter().enumerate() {
            let (target_x, target_y) = seed.targets[i];
            worst_drift = worst_drift.max((x - target_x).abs().max((y - target_y).abs()));
        }
        assert!(
            worst_drift <= drift_tol,
            "node drifted {worst_drift} > {drift_tol} from its seed target \
             (SECTIONS_TETHER {SECTIONS_TETHER} + one lane gap of separation slack)"
        );

        // Non-overlap is a hard user requirement ("space should be chosen so that
        // circuits do not overlap"), so it is asserted on the scale fixture too,
        // not merely "no NaN". Tolerance SECTIONS_LANE_GAP = 40 world units, the
        // same slack 3.8's distinct-lane test used: the wrapped seed itself is clear
        // with margin (measured worst seed penetration −28.00) and the bounded
        // springs plus the overlap-removal pass leave 1.05 (measured) — the
        // tolerance is slack, and it stays far below the 79.19 units of penetration
        // the unbounded anchor pull and untamed cable drag produced before 3.9, so
        // this assertion fails as soon as either bound is removed.
        assert_pairs_clear(&sizes, &positions, SECTIONS_LANE_GAP, "own_scale sections");
        let worst = worst_penetration(&sizes, &positions);
        assert!(
            worst <= SECTIONS_LANE_GAP / 4.0,
            "own_scale worst pairwise penetration {worst} should be slack-level, \
             not near the {SECTIONS_LANE_GAP}-unit tolerance"
        );

        // The layered force seed now shares the same proportionality contract
        // (force-mode-seed-cap 1.1): `seed_positions` dense-normalizes the
        // topological depth before multiplying by HORIZONTAL_SPACING, so on this
        // cyclic fixture it no longer explodes. Before the cap the raw seed
        // spanned 245,281 × 80 = 19,622,480 units (and force mode's solved bbox
        // measured 19,621,300); the dense seed instead spans one spacing unit per
        // distinct layer — 320 here (5 distinct depths) — which is *narrower* than
        // the sections bound, hence the inverted guard this replaces ("force mode
        // still explodes") is no longer the true claim. Asserted instead: the
        // force seed obeys the layer-proportional bound.
        let force_seed = seed_positions(&graph);
        let (force_seed_width, _) = positions_bbox(&force_seed);
        let force_seed_bound = (distinct_layer_count(&graph) - 1) as f32 * HORIZONTAL_SPACING * 1.5;
        assert!(
            force_seed_width <= force_seed_bound,
            "force mode's dense-normalized seed must stay proportional to the layer \
             count: {force_seed_width} > {force_seed_bound}; the raw-depth seed \
             measured 19,622,480 here"
        );
    }
    #[test]
    fn sections_lane_width_is_capped_at_the_exported_max() {
        // The lane-width cap (task 3.8) is exported for the renderer's frame clamp
        // (task 3.9). A node wider than `SECTIONS_LANE_MAX_NODE_W` contributes only
        // the cap to its lane, so one 496-wide body cannot shove the rest of the
        // row out of square: lane 0 is credited 240, so lane 1 starts at
        // 240 + SECTIONS_LANE_GAP = 280 rather than at 496 + 40 = 536.
        let graph = Graph {
            nodes: vec![node(&"a".repeat(60), 0), node("b", 1)],
            ..Default::default()
        };
        let real_w = estimated_widths(&graph)[0];
        assert_eq!(
            real_w, 496.0,
            "60-char title must estimate at 496 before capping"
        );
        assert!(
            real_w > SECTIONS_LANE_MAX_NODE_W,
            "the fixture node must exceed the export cap ({real_w} > {SECTIONS_LANE_MAX_NODE_W})"
        );

        // Lane 1 starts one capped lane plus one lane gap to the right, not one
        // uncapped node width (536) to the right.
        let second_slot_x = SECTIONS_LANE_MAX_NODE_W + SECTIONS_LANE_GAP;
        let lanes = sections_lanes(&graph);
        assert_eq!(lanes.slot, vec![(0.0, 0.0), (second_slot_x, 0.0)]);
        // The seed the solver starts from sits on those slots (both nodes are the
        // only member of their lane, so both are at their lane's top edge).
        assert_eq!(
            seed_positions_sections(&graph),
            vec![(0.0, 0.0), (second_slot_x, 0.0)]
        );
    }

    #[test]
    fn sections_mode_wide_node_lanes_do_not_overlap() {
        // Width-tracking contract (task 3.8): a lane reserves its widest member's
        // `min(estimated_width, SECTIONS_LANE_MAX_NODE_W)`, so two nodes in
        // *distinct* lanes are separated by `lane_w(left) + SECTIONS_LANE_GAP`,
        // which exceeds their required center distance
        // (`0.5 * (max(w_a, 80) + max(w_b, 80))`) for every width up to the cap. A
        // fixed pitch could not do that: with the old `HORIZONTAL_SPACING = 80` the
        // 224/240 pair would need 232 of separation and get 120.
        //
        // Fixture: one node per lane, lanes 0..5, titles of 10/14/18/22/26/28 chars
        // → widths 96/128/160/192/224/240 — all wider than `HORIZONTAL_SPACING`,
        // all at or below the cap, so lane width == node width.
        let graph = Graph {
            nodes: vec![
                node(&"a".repeat(10), 0),
                node(&"b".repeat(14), 1),
                node(&"c".repeat(18), 2),
                node(&"d".repeat(22), 3),
                node(&"e".repeat(26), 4),
                node(&"f".repeat(28), 5),
            ],
            ..Default::default()
        };
        let sizes = node_world_sizes(&graph);
        let widths = estimated_widths(&graph);
        assert_eq!(widths, vec![96.0, 128.0, 160.0, 192.0, 224.0, 240.0]);
        assert!(
            widths.iter().all(|&w| w > HORIZONTAL_SPACING),
            "every fixture node must be wider than the old fixed pitch"
        );
        assert!(
            widths.iter().all(|&w| w <= SECTIONS_LANE_MAX_NODE_W),
            "and inside the cap, so lane width tracks the node exactly"
        );

        let lanes = sections_lanes(&graph);
        assert_eq!(lanes.lane, vec![0, 1, 2, 3, 4, 5], "one node per lane");
        // Slot pitch tracks width: 96 + 40, then 128 + 40, ...
        let expected: Vec<f32> = widths
            .iter()
            .scan(0.0f32, |row_x, &w| {
                let here = *row_x;
                *row_x += w + SECTIONS_LANE_GAP;
                Some(here)
            })
            .collect();
        assert_eq!(lanes.slot.iter().map(|s| s.0).collect::<Vec<_>>(), expected);

        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);

        // Non-overlap: two axis-aligned bodies are clear when separated on the x
        // *or* the y axis. Tolerance `OVERLAP_TOL = SECTIONS_LANE_GAP` (40 world
        // units). Measured penetration is 0.0 for every pair on this fixture, so
        // the tolerance is pure slack — but it stays far below the 112 units of
        // penetration a fixed 80-wide pitch would produce for the 224/240 pair, so
        // the assertion fails as soon as lane width stops tracking node width.
        const OVERLAP_TOL: f32 = SECTIONS_LANE_GAP;
        for a in 0..positions.len() {
            for b in a + 1..positions.len() {
                let dx = (positions[a].0 - positions[b].0).abs();
                let dy = (positions[a].1 - positions[b].1).abs();
                let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
                let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
                assert!(
                    dx >= need_x - OVERLAP_TOL || dy >= need_y - OVERLAP_TOL,
                    "nodes {a} and {b} overlap: |dx|={dx} (need {need_x}), \
                     |dy|={dy} (need {need_y}), tolerance {OVERLAP_TOL}"
                );
            }
        }
    }

    #[test]
    fn sections_mode_lanes_wrap_into_rows_in_patch_order() {
        // Wrap contract (task 3.8): lanes pack left→right while
        // `row_x + lane_w ≤ SECTIONS_ROW_TARGET_W`, then the packer wraps to a new
        // row and continues with the next lane index. This fixture is 20 lanes of
        // exactly cap-width nodes (28 chars → 240 world units), so a lane costs
        // 240 + SECTIONS_LANE_GAP = 280: lanes 0..13 fill row 0
        // (13·280 + 240 = 3880 ≤ 4000) and lane 14 wraps (14·280 + 240 = 4160 >
        // 4000), so lanes 14..19 continue on row 1 starting again at x = 0. Row
        // pitch is NODE_WORLD_H + SECTIONS_ROW_GAP = 160.
        let graph = Graph {
            nodes: (0..20).map(|i| node(&"w".repeat(28), i)).collect(),
            ..Default::default()
        };
        let lanes = sections_lanes(&graph);
        assert_eq!(lanes.slot.len(), 20);
        let row_0 = &lanes.slot[..14];
        let row_1 = &lanes.slot[14..];
        assert!(
            row_0.iter().all(|s| s.1 == 0.0) && row_1.iter().all(|s| s.1 == 160.0),
            "lanes 0..13 must sit in row 0 (y = 0) and lanes 14..19 in row 1 \
             (y = 160), got {:?}",
            lanes.slot
        );
        assert_eq!(row_0[0], (0.0, 0.0));
        assert_eq!(row_0[13], (3640.0, 0.0)); // 13 · 280
        assert_eq!(row_1[0], (0.0, 160.0)); // wrapped: restarts at row x = 0

        // Ascending lane order inside each row, then the next row below.
        for pair in row_0.windows(2).chain(row_1.windows(2)) {
            assert!(
                pair[0].0 < pair[1].0,
                "lanes must ascend left→right within a row"
            );
        }
        assert!(row_1[0].1 > row_0[13].1, "row 1 must sit below row 0");

        // The solve keeps the wrap: two tight row y-bands, ordered, with a real
        // gap. The band y is not the slot y — a 240-wide body's extent is 240 while
        // the row pitch is only 160, so the rows repel vertically (observed row 0 at
        // −30.60, row 1 at 190.60: 221.2 apart, well above SECTIONS_ROW_GAP = 80).
        // What the contract needs is that each row stays one band and the bands stay
        // ordered; the per-node x must stay on its lane slot.
        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);
        let y_band = |ps: &[(f32, f32)]| {
            let lo = ps.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let hi = ps.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
            (lo, hi)
        };
        let (row_0_lo, row_0_hi) = y_band(&positions[..14]);
        let (row_1_lo, row_1_hi) = y_band(&positions[14..]);
        assert!(
            row_0_hi - row_0_lo <= SECTIONS_ROW_GAP,
            "row 0 must stay one y-band: {row_0_lo}..{row_0_hi} spans more than \
             SECTIONS_ROW_GAP = {SECTIONS_ROW_GAP}"
        );
        assert!(
            row_1_hi - row_1_lo <= SECTIONS_ROW_GAP,
            "row 1 must stay one y-band: {row_1_lo}..{row_1_hi} spans more than \
             SECTIONS_ROW_GAP = {SECTIONS_ROW_GAP}"
        );
        assert!(
            row_1_lo - row_0_hi > SECTIONS_ROW_GAP,
            "the two rows must stay separate: row 0 top {row_0_hi}, row 1 bottom \
             {row_1_lo}"
        );
        for (i, &(x, _)) in positions.iter().enumerate() {
            assert!(
                (x - lanes.slot_of(i).0).abs() <= SECTIONS_LANE_GAP,
                "node {i} x={x} left its lane slot {}",
                lanes.slot_of(i).0
            );
        }
    }

    /// Worst pairwise body penetration over `positions`, in world units. Two
    /// axis-aligned bodies are clear once `|Δx| ≥ (w_a + w_b) / 2` *or*
    /// `|Δy| ≥ (h_a + h_b) / 2`, so the penetration of a pair is the smaller of
    /// the two shortfalls: `≤ 0` means the pair is clear.
    fn worst_penetration(sizes: &[(f32, f32)], positions: &[(f32, f32)]) -> f32 {
        let mut worst = f32::NEG_INFINITY;
        for a in 0..positions.len() {
            for b in a + 1..positions.len() {
                let dx = (positions[a].0 - positions[b].0).abs();
                let dy = (positions[a].1 - positions[b].1).abs();
                let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
                let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
                worst = worst.max((need_x - dx).min(need_y - dy));
            }
        }
        worst
    }

    /// Assert every pair of bodies is clear within `tol` world units.
    fn assert_pairs_clear(sizes: &[(f32, f32)], positions: &[(f32, f32)], tol: f32, what: &str) {
        for a in 0..positions.len() {
            for b in a + 1..positions.len() {
                let dx = (positions[a].0 - positions[b].0).abs();
                let dy = (positions[a].1 - positions[b].1).abs();
                let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
                let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
                let penetration = (need_x - dx).min(need_y - dy);
                assert!(
                    penetration <= tol,
                    "{what}: nodes {a} and {b} overlap: |dx|={dx} (need {need_x}), \
                     |dy|={dy} (need {need_y}), penetration {penetration} > {tol}"
                );
            }
        }
    }

    /// Fixture for the 3.9 non-overlap contract: one lane holding a three-node
    /// stack, and one banner group whose sixteen wide circuits wrap across a row
    /// boundary.
    ///
    /// Every node is named with a 28-character title so its estimated width is
    /// exactly the exported lane cap ([`SECTIONS_LANE_MAX_NODE_W`] = 240) and every
    /// lane is one cap-wide lane: the packing arithmetic is then exact, and no lane
    /// is narrower than its neighbour (the one seed-level shortfall the wrapped
    /// grid can have — see [`separate_overlaps`]).
    ///
    /// Node layout: 0..2 are the outer-lane stack (two controllers + an input
    /// jack, all `section_index` 0 → lane 0), 3..18 are the sixteen circuits
    /// (`section_index` 1..16 → lanes 1..16, all members of banner group `grp`).
    fn wrapped_stack_graph() -> Graph {
        let long = |name: &str| format!("{name}{}", "w".repeat(28 - name.len()));
        let outer = |name: &str, kind: NodeKind, section_index: usize| GraphNode {
            id: match kind {
                NodeKind::Controller => NodeId::Controller(long(name), 0),
                NodeKind::InputJack => NodeId::Jack(long(name)),
                other => {
                    unreachable!("outer fixture nodes are controllers or input jacks: {other:?}")
                }
            },
            kind,
            circuit: long(name),
            instance_index: 0,
            section_index,
        };
        let mut nodes = vec![
            outer("ctrl1", NodeKind::Controller, 0),
            outer("ctrl2", NodeKind::Controller, 0),
            outer("IN1", NodeKind::InputJack, 0),
        ];
        for section in 1..=16usize {
            nodes.push(node(&long(&format!("c{section}")), section));
        }
        Graph {
            nodes,
            clusters: vec![Cluster {
                title: "grp".to_string(),
                section_range: 1..17,
            }],
            validation: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn sections_mode_stacked_lanes_and_wrapped_clusters_do_not_overlap() {
        // Covers the two residual defects of task 3.8 in one fixture, and asserts
        // the hard requirement they broke (non-overlap) for every pair.
        //
        // Defect A — in-lane stacking. Lane 0 is the outer lane and holds three
        // nodes, so the seed stacks them at NODE_WORLD_H + SECTIONS_STACK_GAP
        // = 120 pitch. The slice spring must target each node's *own* stacked seed
        // y; targeting the lane's single `slot_y` compressed the stack onto one y
        // (pitch 0 before the fix, and only 80 with diagonal help from size-aware
        // repulsion) even though the seed had reserved 120.
        //
        // Defect B — cross-row anchors. Group `grp` owns sixteen 240-wide
        // circuits; lanes 1..13 land on row 0 and lanes 14..16 wrap to row 1 (row 0
        // is as tall as lane 0's stack: 3·80 + 2·40 = 320, plus SECTIONS_ROW_GAP
        // = 80 → row 1's slot y = 400). The group's single global mean anchor
        // would sit at ≈ (1645, 75), ~1300 units from the wrapped members and
        // across a row boundary; per-(group, row) anchors instead give each row
        // band its own anchor — (1960, 0) for row 0 and (280, 400) for row 1.
        let graph = wrapped_stack_graph();
        let seed = sections_seed(&graph);

        // Seed geometry, hand-derived above.
        assert_eq!(
            seed.lanes.lane[..3],
            [0, 0, 0],
            "the outer lane holds the three stacked nodes"
        );
        assert_eq!(seed.lanes.slot[0], (0.0, 0.0));
        assert_eq!(
            seed.lanes.slot[13],
            (3640.0, 0.0),
            "lane 13 is the last lane of row 0"
        );
        assert_eq!(
            seed.lanes.slot[14],
            (0.0, 400.0),
            "lane 14 wraps to row 1, whose y accounts for lane 0's full 320-unit stack"
        );
        assert_eq!(seed.lanes.slot[16], (560.0, 400.0));
        assert_eq!(
            (seed.lanes.row_of(0), seed.lanes.row_of(16)),
            (0, 1),
            "node 0 is the outer lane's stack, node 16 is the first wrapped circuit"
        );
        assert_eq!(
            seed.targets[..3],
            [(0.0, 0.0), (0.0, 120.0), (0.0, 240.0)],
            "a three-node lane stacks at the full 120 pitch"
        );

        let positions = solve_sections(&graph, &[], SolveKnobs::default());
        assert_finite_and_bounded(&positions);

        // Defect A: the stack can no longer collapse below the pitch its lane
        // reserved. The seed stacks lane 0 at NODE_WORLD_H + SECTIONS_STACK_GAP
        // = 120, and size-aware repulsion may spread it further (these synthetic
        // nodes are 240 wide, and the sections force model separates by
        // `max(width, height)`, so their radial requirement is 240 — measured pitch
        // 174.9), but the bounded springs can never pull it back under 120. Before
        // the fix every member targeted the lane's single `slot_y`, so the stack
        // collapsed onto one y and only the overlap-removal pass recreated a gap —
        // at `h_a + h_b` = 80, below the reserved pitch.
        let pitch = NODE_WORLD_H + SECTIONS_STACK_GAP;
        let pitch_ceiling = pitch + 2.0 * SECTIONS_TETHER + GRID_SNAP;
        for i in 0..2 {
            let dy = (positions[i].1 - positions[i + 1].1).abs();
            assert!(
                dy >= pitch - SECTIONS_LANE_GAP / 4.0,
                "stacked lane-0 members {i}/{}: pitch {dy} is below the reserved {pitch}",
                i + 1
            );
            assert!(
                dy <= pitch_ceiling,
                "stacked lane-0 members {i}/{}: pitch {dy} exceeds the tether-bounded \
                 {pitch_ceiling}",
                i + 1
            );
        }

        // Defect B: the wrapped pair stays in its own row band, around its own
        // band anchor. Tolerance SECTIONS_LANE_GAP = 40 world units; with a single
        // global anchor the pull saturates the 60-unit tether and each member
        // leaves both its band's x and its row's y by ~60.
        let wrapped = [16usize, 17, 18]; // circuits in lanes 14..16, i.e. row 1
        let band_mean_x: f32 =
            wrapped.iter().map(|&i| positions[i].0).sum::<f32>() / wrapped.len() as f32;
        let band_target_x: f32 =
            wrapped.iter().map(|&i| seed.targets[i].0).sum::<f32>() / wrapped.len() as f32;
        assert_eq!(band_target_x, 280.0, "the wrapped band's own anchor");
        assert!(
            (band_mean_x - band_target_x).abs() <= SECTIONS_LANE_GAP,
            "wrapped band mean x {band_mean_x} left its own row's anchor {band_target_x}"
        );
        for &i in &wrapped {
            assert!(
                (positions[i].1 - seed.targets[i].1).abs() <= SECTIONS_LANE_GAP,
                "wrapped member {i} y={} left its row (seed y {})",
                positions[i].1,
                seed.targets[i].1
            );
        }

        // The hard requirement (user decision: "space should be chosen so that
        // circuits do not overlap"): every pair is clear on at least one axis.
        // Tolerance SECTIONS_LANE_GAP = 40 world units — the same slack 3.8's
        // distinct-lane test used, justified the same way: the seed reserves a full
        // lane per node and the overlap-removal pass resolves what the bounded
        // springs still produce, so the tolerance is slack rather than allowance.
        // It stays far below the ~120 units of penetration the unbounded anchor
        // pull and the untamed cable drag produced before 3.9.
        let sizes = node_world_sizes(&graph);
        assert_pairs_clear(
            &sizes,
            &positions,
            SECTIONS_LANE_GAP,
            "wrapped-stack fixture",
        );
        let worst = worst_penetration(&sizes, &positions);
        assert!(
            worst <= SECTIONS_LANE_GAP / 4.0,
            "the worst pairwise penetration {worst} should be slack-level, not near \
             the {SECTIONS_LANE_GAP}-unit tolerance"
        );

        // Determinism survives the bounded pulls and the separation pass.
        assert_eq!(
            positions,
            solve_sections(&graph, &[], SolveKnobs::default()),
            "the bounded sections solve must be deterministic"
        );
    }

    // ── force-arrangement-nonoverlap-and-sliders: tasks 3.1 / 3.2 ──────────
    //
    // The force path's contract is a pair of invariants (design D8):
    // **non-overlap** as a post-condition of every path that produces positions,
    // and **determinism per knob tuple**. Tasks 1.4/1.5 measured both with a
    // throwaway harness; these tests make the measurements permanent.
    //
    // The overlap predicate here is the test module's own (`assert_pairs_clear` /
    // `worst_penetration`), not the solver's `count_overlapping_pairs`: if the
    // solver's predicate drifted, an invariant asserted through it would pass
    // vacuously.

    /// The fixture set the force non-overlap invariant is pinned on (task 3.1).
    /// `own_scale` is the 511-body anchor and the binding fixture for the
    /// default cohesion (18 separation sweeps, design D3); the other four are the
    /// rest of the measured set.
    const FORCE_NONOVERLAP_FIXTURES: &[&str] = &[
        "fixtures/own_scale.ini",
        "fixtures/cable_banner_combos.ini",
        "fixtures/graph_latency_chain.ini",
        "fixtures/own_two_ctrl.ini",
        "fixtures/graph_dependency_walk.ini",
    ];

    /// The DROID patch the cancelled recon lane listed. It is not part of
    /// `fixtures/`, so it is only exercised when a checkout carries a copy
    /// there — the same presence guard the measurement probe used.
    const FORCE_NONOVERLAP_OPTIONAL_FIXTURE: &str = "fixtures/droid_mpfs5melody2.ini";

    /// Closest pair of node positions; ties break to the lower index pair.
    fn closest_node_pair(positions: &[(f32, f32)]) -> (usize, usize) {
        let mut best: Option<(f32, usize, usize)> = None;
        for a in 0..positions.len() {
            for b in a + 1..positions.len() {
                let d = dist(positions[a], positions[b]);
                if best.map(|(best_d, _, _)| d < best_d).unwrap_or(true) {
                    best = Some((d, a, b));
                }
            }
        }
        let (_, a, b) = best.expect("closest pair needs at least two nodes");
        (a, b)
    }

    /// Mean Euclidean distance from every node in `members` to the centroid of
    /// `members` — the cluster-tightness measure the cohesion knob is pinned on.
    fn mean_distance_to_centroid(positions: &[(f32, f32)], members: &[usize]) -> f32 {
        assert!(!members.is_empty(), "centroid needs members");
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        for &i in members {
            sx += positions[i].0;
            sy += positions[i].1;
        }
        let centroid = (sx / members.len() as f32, sy / members.len() as f32);
        members
            .iter()
            .map(|&i| dist(positions[i], centroid))
            .sum::<f32>()
            / members.len() as f32
    }

    /// Mean pairwise distance over `members` — the layout-spread measure the
    /// repulsion gain is pinned on.
    fn mean_pairwise_distance(positions: &[(f32, f32)], members: &[usize]) -> f32 {
        let mut total = 0.0f32;
        let mut count = 0usize;
        for a in 0..members.len() {
            for b in a + 1..members.len() {
                total += dist(positions[members[a]], positions[members[b]]);
                count += 1;
            }
        }
        total / count as f32
    }

    /// Assert every pair of bodies whose **both** endpoints are marked active is
    /// clear within `tol` world units — the scope a drag's separation pass can
    /// guarantee (design D4).
    ///
    /// The stronger "at least one endpoint active" reading is deliberately *not*
    /// asserted, because it is not achievable: the active set is movable while
    /// everything outside it is a fixed anchor, and an active body wedged between
    /// two anchors closer together than its own required separation cannot be
    /// clear of both. Measured on `fixtures/own_scale.ini` (2026-10-15): dropping
    /// the nearest node onto its partner leaves 6 residual active/inactive pairs
    /// that survive an 8192-sweep separation unchanged — a cage, not a sweep
    /// budget — while every active/active pair is clear.
    fn assert_active_pairs_clear(
        sizes: &[(f32, f32)],
        positions: &[(f32, f32)],
        active: &[bool],
        tol: f32,
        what: &str,
    ) {
        for a in 0..positions.len() {
            for b in a + 1..positions.len() {
                if !active[a] || !active[b] {
                    continue;
                }
                let dx = (positions[a].0 - positions[b].0).abs();
                let dy = (positions[a].1 - positions[b].1).abs();
                let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
                let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
                let penetration = (need_x - dx).min(need_y - dy);
                assert!(
                    penetration <= tol,
                    "{what}: nodes {a} and {b} overlap: |dx|={dx} (need {need_x}), \
                     |dy|={dy} (need {need_y}), penetration {penetration} > {tol}"
                );
            }
        }
    }

    #[test]
    fn force_solve_has_zero_overlapping_pairs_on_fixture_set() {
        // Task 3.1 / design D3: `solve` returns zero overlapping axis-aligned
        // body pairs on the measured fixture set under the task 1.5 defaults.
        // Measured (2026-10-15): all five fixtures reach zero pairs, the anchor
        // at 18 sweeps; this makes that fact executable.
        let mut checked = 0usize;
        for name in FORCE_NONOVERLAP_FIXTURES {
            if !Path::new(name).exists() {
                continue;
            }
            let graph = graph_from_fixture(name);
            assert!(!graph.nodes.is_empty(), "{name}: fixture graph is empty");
            let sizes = node_world_sizes(&graph);
            let positions = solve(&graph, &[], SolveKnobs::default());
            assert_finite_and_bounded(&positions);
            assert_pairs_clear(&sizes, &positions, 0.0, name);
            checked += 1;
        }
        assert!(
            checked == FORCE_NONOVERLAP_FIXTURES.len(),
            "the whole measured fixture set must be present, checked {checked} of {}",
            FORCE_NONOVERLAP_FIXTURES.len()
        );

        if Path::new(FORCE_NONOVERLAP_OPTIONAL_FIXTURE).exists() {
            let graph = graph_from_fixture(FORCE_NONOVERLAP_OPTIONAL_FIXTURE);
            let sizes = node_world_sizes(&graph);
            let positions = solve(&graph, &[], SolveKnobs::default());
            assert_finite_and_bounded(&positions);
            assert_pairs_clear(&sizes, &positions, 0.0, FORCE_NONOVERLAP_OPTIONAL_FIXTURE);
        }
    }

    #[test]
    fn local_resettle_clears_overlap_in_the_active_neighbourhood() {
        // Task 3.1 / design D4: after a deliberately overlapping drag, no two
        // *active* bodies overlap. That is the scope the pass can guarantee: every
        // body outside `LOCAL_RADIUS` is a fixed anchor, so a pair of two inactive
        // nodes never changed (the pre-drag solve left it clear) and an
        // active/inactive pair can be unresolvable when the active body is wedged
        // between anchors — the cage case measured and documented on
        // [`assert_active_pairs_clear`].
        //
        // The deliberate overlap: drop the wider member of the closest pair
        // exactly onto its partner, i.e. a total body overlap at the centre of
        // the neighbourhood.
        for name in FORCE_NONOVERLAP_FIXTURES {
            if !Path::new(name).exists() {
                continue;
            }
            let graph = graph_from_fixture(name);
            let sizes = node_world_sizes(&graph);
            let mut positions = solve(&graph, &[], SolveKnobs::default());

            let (a, b) = closest_node_pair(&positions);
            let (moved, target) = if sizes[a].0 >= sizes[b].0 {
                (a, b)
            } else {
                (b, a)
            };
            positions[moved] = positions[target];
            // The active set `local_resettle` computes: distance to the moved
            // node's position *at call time*.
            let center = positions[moved];
            let active: Vec<bool> = positions
                .iter()
                .map(|&p| dist(p, center) <= LOCAL_RADIUS)
                .collect();
            assert!(
                active[moved] && active[target],
                "{name}: the deliberate overlap must sit inside the active neighbourhood"
            );

            let moved_id = graph.nodes[moved].id.clone();
            assert!(local_resettle(
                &graph,
                &mut positions,
                &moved_id,
                LOCAL_RADIUS,
                LOCAL_ITERATIONS,
                &[],
                SolveKnobs::default(),
            ));

            assert_active_pairs_clear(
                &sizes,
                &positions,
                &active,
                0.0,
                &format!("{name}: active neighbourhood after local_resettle"),
            );
        }
    }

    #[test]
    fn columns_pinned_with_overlapping_off_grid_pins_has_no_overlap() {
        // Task 3.1 / design D5. Two pins carried over from a force solve or a
        // drag can land on top of each other, off the slot grid, and two
        // immovable bodies cannot be separated — so exact pin honoring yields to
        // the no-overlap invariant: phase 1 keeps every pin fixed (unpinned
        // neighbours give way), then one all-movable pass separates the pins.
        let graph = make_graph(2, &[]);
        let widths = vec![20.0; 2];
        let pins = [(0, (10.0, 60.0)), (1, (10.0, 60.0))];
        let positions = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &pins);
        let sizes: Vec<(f32, f32)> = widths.iter().map(|&w| (w, NODE_WORLD_H)).collect();
        assert_finite_and_bounded(&positions);
        assert_pairs_clear(
            &sizes,
            &positions,
            0.0,
            "column solve with two coincident off-grid pins",
        );
        assert!(
            positions[0] != (10.0, 60.0) || positions[1] != (10.0, 60.0),
            "two intersecting pinned bodies cannot both keep their requested position: {positions:?}"
        );

        // A lone off-grid pin has no pinned partner to conflict with, so exact
        // pin honoring stands: the unpinned neighbours give way and the pin does
        // not move at all. This guards the D5 trade-off from over-reaching.
        let graph = make_graph(6, &[(0, 5), (1, 5), (2, 5), (3, 5), (4, 5)]);
        let widths = vec![20.0; 6];
        let lone = solve_columns_pinned(
            &graph,
            &widths,
            LayoutOrdering::Strict,
            &[(1, (10.0, 60.0))],
        );
        assert_eq!(
            lone[1],
            (10.0, 60.0),
            "a lone off-grid pin must keep its exact position"
        );
        let sizes: Vec<(f32, f32)> = widths.iter().map(|&w| (w, NODE_WORLD_H)).collect();
        assert_pairs_clear(&sizes, &lone, 0.0, "column solve with one off-grid pin");
    }

    #[test]
    fn solve_knobs_default_reproduces_the_documented_defaults() {
        // Task 3.2: `SolveKnobs` is the determinism key, so its default must be
        // the documented tuple (design D6) — and the two force-mode values it
        // carries are the *measured* ones (design D3): the gain that reproduces
        // the tuned forces, and the cohesion bracketed from above by the
        // zero-pairs band and from below by the cluster-coherence floor.
        let knobs = SolveKnobs::default();
        assert_eq!(knobs.tension, DEFAULT_TENSION);
        assert_eq!(knobs.slice_k, SLICE_K);
        assert_eq!(knobs.section_k, SECTION_HOLE_K);
        assert_eq!(knobs.repulsion, REPULSION_GAIN_DEFAULT);
        assert_eq!(knobs.cohesion, COHESION_K);
        assert_eq!(DEFAULT_TENSION, SPRING_K, "documented tension default");
        assert_eq!(REPULSION_GAIN_DEFAULT, 1.0, "documented gain default");
        assert_eq!(
            COHESION_K, 0.0068,
            "the measured cohesion default (design D3's table)"
        );
    }

    /// Graph for the knob-effect tests: one source feeding five sinks (cable
    /// springs respond to `tension`), every node in one banner group (cohesion
    /// pulls the sinks together), and long titles so the sink bodies are wide
    /// enough for the repulsion ramp to fire. All three force-mode knobs
    /// therefore have a mechanism to act on.
    fn knob_probe_graph() -> Graph {
        let mut graph = make_graph(6, &[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
        for (i, node) in graph.nodes.iter_mut().enumerate() {
            node.circuit = format!("sink{i:02}{}", "w".repeat(20));
        }
        graph.clusters = vec![Cluster {
            title: "grp".to_string(),
            section_range: 0..6,
        }];
        graph
    }

    /// Nine nodes in one banner group, no edges, seeded one [`VERTICAL_SPACING`]
    /// apart in a single column — the cluster-tightness fixture for the cohesion
    /// knob. The group is wide enough (a 266-unit mean radius at zero cohesion)
    /// that the pull is a smooth contraction across the knob's live range instead
    /// of a step at the collapse threshold: measured mean distance to the group
    /// centroid 266.68 / 238.36 / 211.88 / 179.58 for cohesion
    /// 0 / 0.002 / 0.004 / [`COHESION_K`].
    fn one_cluster_graph() -> Graph {
        let nodes: Vec<GraphNode> = (0..9).map(|i| node(&format!("n{i}"), i)).collect();
        Graph {
            nodes,
            edges: vec![],
            clusters: vec![Cluster {
                title: "one".to_string(),
                section_range: 0..9,
            }],
            validation: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn solve_knobs_tuple_is_the_determinism_key() {
        // Task 3.2: same graph + same knob tuple → identical positions, asserted
        // on a *non-default* tuple so the key is the whole bundle rather than the
        // defaults.
        let graph = knob_probe_graph();
        let knobs = SolveKnobs {
            tension: 0.2,
            repulsion: 1.4,
            cohesion: 0.004,
            ..SolveKnobs::default()
        };
        let first = solve(&graph, &[], knobs);
        assert_finite_and_bounded(&first);
        assert_eq!(
            first,
            solve(&graph, &[], knobs),
            "identical knob tuples must reproduce identical positions"
        );
    }

    #[test]
    fn each_force_knob_measurably_changes_the_layout() {
        // Task 3.2: no force-mode knob may silently become a no-op. Tension's own
        // test (`tension_is_deterministic_per_value_and_changes_layout`) covers it
        // on a plain graph; this triple-checks all three on one graph that gives
        // every knob a mechanism.
        let graph = knob_probe_graph();
        let base = SolveKnobs {
            tension: 0.2,
            repulsion: 1.4,
            cohesion: 0.004,
            ..SolveKnobs::default()
        };
        let baseline = solve(&graph, &[], base);
        for (label, knobs) in [
            (
                "tension",
                SolveKnobs {
                    tension: 0.35,
                    ..base
                },
            ),
            (
                "repulsion",
                SolveKnobs {
                    repulsion: 0.5,
                    ..base
                },
            ),
            (
                "cohesion",
                SolveKnobs {
                    cohesion: 0.006,
                    ..base
                },
            ),
        ] {
            let changed = solve(&graph, &[], knobs);
            assert_ne!(baseline, changed, "changing {label} must change the layout");
        }
    }

    #[test]
    fn repulsion_gain_changes_layout_spread() {
        // Task 3.2: the repulsion gain has a measurable geometric effect, not
        // just a different float. On the knob-probe graph the ramp pushes the five
        // wide sinks apart while cohesion pulls them together, so their
        // equilibrium spread grows with the gain; the measure is the mean pairwise
        // distance, so a gain that only reshuffled nodes without spreading them
        // would fail.
        let graph = knob_probe_graph();
        let sinks: Vec<usize> = (1..6).collect();
        let spread = |repulsion: f32| {
            let positions = solve(
                &graph,
                &[],
                SolveKnobs {
                    repulsion,
                    ..SolveKnobs::default()
                },
            );
            assert_finite_and_bounded(&positions);
            mean_pairwise_distance(&positions, &sinks)
        };
        let off = spread(0.0);
        let nominal = spread(REPULSION_GAIN_DEFAULT);
        let strong = spread(4.0);
        assert!(
            nominal > off,
            "the ramp must spread the sinks: {nominal} !> {off} (gain off)"
        );
        assert!(
            strong > nominal,
            "a higher gain must spread further: {strong} !> {nominal}"
        );
    }

    #[test]
    fn cohesion_gain_tightens_banner_clusters() {
        // Task 3.2 / design D3: the cohesion gain has a measurable effect on
        // cluster tightness. The fixture's only forces are the group's own
        // cohesion, the repulsion falloff, and the separation post-condition, so
        // raising the gain must contract the group — and it must do so across the
        // knob's live range, not just at one point (a single-point assertion
        // would not catch a knob that only bites past its threshold).
        let graph = one_cluster_graph();
        let members: Vec<usize> = (0..9).collect();
        let tightness = |cohesion: f32| {
            let positions = solve(
                &graph,
                &[],
                SolveKnobs {
                    cohesion,
                    ..SolveKnobs::default()
                },
            );
            assert_finite_and_bounded(&positions);
            mean_distance_to_centroid(&positions, &members)
        };
        let ladder: Vec<f32> = [0.0, 0.002, 0.004, COHESION_K]
            .iter()
            .copied()
            .map(tightness)
            .collect();
        for pair in ladder.windows(2) {
            assert!(
                pair[1] < pair[0],
                "cohesion must tighten the group across its live range, got {ladder:?}"
            );
        }
        // The default's effect is substantial, not numerical dust: measured 32%
        // tighter than zero cohesion, asserted at half that margin.
        assert!(
            ladder[3] < ladder[0] * 0.8,
            "the default cohesion must tighten the group measurably: {} !< {}",
            ladder[3],
            ladder[0] * 0.8
        );
    }
}
