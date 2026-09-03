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
//! Quad-view usage: `solve` is the single convergence entry point for both
//! the FULL graph and the FILTERED induced subgraph. The FILTERED pane holds
//! `filtered_positions = solve(&filtered_graph)` independently from
//! `graph_positions = solve(&full_graph)` — a fresh compact solve, not a
//! reuse of FULL positions. `solve_filtered` is a thin alias for that call
//! site so the intent is explicit and tests can target the filtered path
//! without coupling to FULL-graph fixtures.

use std::collections::HashMap;

use crate::graph::{Graph, NodeId};

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
/// Repulsion cutoff radius; also the uniform-grid cell size.
const REPULSION_RADIUS: f32 = 120.0;
/// Per-axis cap on a single velocity update, keeps the solver from exploding.
const MAX_DISPLACEMENT: f32 = 20.0;

/// Weak per-cluster cohesion pulling each member toward its banner group's
/// centroid (design D3). Deliberately far below `SPRING_K` so clusters
/// cohere without collapsing or overriding the single-axis spring bias.
const COHESION_K: f32 = 0.01;

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
/// Used for both the FULL graph (`solve(&full_graph)`) and the FILTERED
/// induced subgraph (`solve(&filtered_graph)` / `solve_filtered`) — each call
/// seeds deterministically from its own graph (topological depth + within-
/// layer order + node-id hash, no RNG) and converges independently under the
/// same bounded, grid-hashed solver (`SOLVE_ITERATIONS`, `ENERGY_THRESHOLD`).
///
/// `tension` is the spring stiffness (`SPRING_K`); pass [`DEFAULT_TENSION`]
/// for the tuned default. Higher tension pulls cable-connected nodes closer
/// together, lower tension lets repulsion spread them out. Determinism holds
/// per tension value: same patch + same machine + same tension → same layout.
pub fn solve(graph: &Graph, pinned: &[usize], tension: f32) -> Vec<(f32, f32)> {
    let mut positions = seed_positions(graph);
    run_iterations(
        graph,
        &mut positions,
        SOLVE_ITERATIONS,
        None,
        pinned,
        tension,
    );
    positions
}

/// Compact convergence for the FILTERED quad pane (task 2.2).
///
/// Thin alias over [`solve`] — identical bounded, deterministic, grid-hashed
/// convergence (`SOLVE_ITERATIONS` cap, `ENERGY_THRESHOLD` freeze, no RNG).
/// Exists as a distinct entry point so the quad view can hold
/// `filtered_positions = solve_filtered(&filtered_graph)` independently from
/// `graph_positions = solve(&full_graph)`, and so filtered-compact tests can
/// target this path without coupling to FULL-graph fixtures. A small filtered
/// graph converges compactly and quickly; when the filtered set is a strict
/// subset of FULL, its positions differ from the corresponding FULL subset.
/// `pinned` behaves exactly as in [`solve`]; pass `&[]` for an unpinned
/// filtered solve. `tension` behaves exactly as in [`solve`]. Pure, no
/// terminal dependency.
pub fn solve_filtered(graph: &Graph, pinned: &[usize], tension: f32) -> Vec<(f32, f32)> {
    solve(graph, pinned, tension)
}

/// Damped local re-settle after a node move (design D1).
///
/// Only nodes within `radius` of the moved node are active and move; distant
/// nodes act as anchors and stay essentially unmoved. `iterations` should be
/// well below `SOLVE_ITERATIONS` (default `LOCAL_ITERATIONS`). `moved` must be a
/// node in `graph`; positions are otherwise left untouched. `pinned` marks
/// node indices that never move — they stay exactly where they currently sit
/// (e.g. a drag-dropped anchor) while still exerting forces on the active
/// neighbourhood. Returns whether the moved node was found. `tension` is the
/// spring stiffness used for the re-settle (see [`solve`]).
pub fn local_resettle(
    graph: &Graph,
    positions: &mut [(f32, f32)],
    moved: &NodeId,
    radius: f32,
    iterations: usize,
    pinned: &[usize],
    tension: f32,
) -> bool {
    let Some(center) = graph.nodes.iter().position(|n| &n.id == moved) else {
        return false;
    };
    let c = positions[center];
    let active: Vec<bool> = (0..graph.nodes.len())
        .map(|i| dist(positions[i], c) <= radius)
        .collect();
    run_iterations(graph, positions, iterations, Some(&active), pinned, tension);
    true
}

/// Deterministic initial placement along a single dominant left→right axis
/// (design D1): sources on the left, sinks right. The seed is a layered
/// (Sugiyama-style) arrangement — `x = depth · HORIZONTAL_SPACING`,
/// `y = within-layer order · VERTICAL_SPACING` — where the within-layer order
/// comes from deterministic barycenter crossing-minimization sweeps
/// ([`crossing_minimized_orders`]) instead of file order. Coordinates land
/// exactly on the [`GRID_SNAP`] grid (no jitter), so the convergence target is
/// a clean horizontal chain of aligned columns.
pub(crate) fn seed_positions(graph: &Graph) -> Vec<(f32, f32)> {
    let n = graph.nodes.len();
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);

    // Longest-path topological depth via bounded Bellman-Ford relaxation. The
    // n-pass bound keeps cyclic graphs finite: each pass relaxes every edge
    // once, so on a cycle depth grows by at most the cycle length per pass and
    // can exceed n — the per-depth map below must not assume depth < n.
    let mut depth = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for &(u, v) in &edges {
            if depth[v] < depth[u] + 1 {
                depth[v] = depth[u] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Crossing-minimized within-layer order (barycenter sweeps, deterministic).
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

/// Deterministic within-layer ordering that reduces edge crossings between
/// adjacent layers (barycenter heuristic, design gv5).
///
/// Nodes are grouped into layers by longest-path depth. A fixed number of
/// sweeps alternates left→right and right→left; in each sweep every layer is
/// re-ordered by the mean (barycenter) of its neighbors' current orders in
/// the adjacent layer, using a stable sort so equal barycenters keep their
/// current relative order. Nodes with no neighbor in the adjacent layer keep
/// their current order. No RNG and a fixed sweep count make the result
/// deterministic: same graph → same order.
fn crossing_minimized_orders(graph: &Graph, depth: &[usize]) -> Vec<usize> {
    let n = graph.nodes.len();
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);

    // Group node indices by depth into layers, preserving file order within
    // each layer. Depth can exceed `n` on cyclic graphs, so layers live in a
    // map keyed by depth, not a fixed-size vec.
    let mut layers: Vec<Vec<usize>> = Vec::new();
    let mut layer_of_depth: HashMap<usize, usize> = HashMap::new();
    for (i, &d) in depth.iter().enumerate().take(n) {
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
            for &(u, v) in &edges {
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

    order
}

/// Advance `positions` up to `max_iter` iterations, returning how many were
/// actually run so callers can detect early energy-threshold convergence.
/// `active` (when `Some`) marks which nodes may move; inactive nodes stay put
/// but still exert forces (they anchor the active set). `pinned` marks node
/// indices that never move, exactly like inactive nodes but for the whole
/// solve (fixed anchors; out-of-range indices are ignored). Freezes early
/// when kinetic energy converges.
fn run_iterations(
    graph: &Graph,
    positions: &mut [(f32, f32)],
    max_iter: usize,
    active: Option<&[bool]>,
    pinned: &[usize],
    tension: f32,
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
    // Cluster membership for cohesion (design D3); `None` = node in no banner
    // group, so the cohesion force is skipped defensively for it.
    let member_of: Vec<Option<usize>> = (0..n)
        .map(|i| graph.cluster_index_of(graph.nodes[i].section_index))
        .collect();
    let mut velocity = vec![(0.0f32, 0.0f32); n];

    let mut iterations = 0;
    for _ in 0..max_iter {
        iterations += 1;
        // Rebuild the uniform repulsion grid from current positions (D9).
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, &pos) in positions.iter().enumerate().take(n) {
            grid.entry(cell_of(pos)).or_default().push(i);
        }

        let mut accel = vec![(0.0f32, 0.0f32); n];

        for &(u, v) in &edges {
            let m = edge_multiplicity[&(u, v)] as f32;
            let f = spring_force(positions[u], positions[v], tension);
            accel[u].0 += f.0 / m;
            accel[u].1 += f.1 / m;
            accel[v].0 -= f.0 / m;
            accel[v].1 -= f.1 / m;
        }

        for (i, &pos) in positions.iter().enumerate().take(n) {
            let (cx, cy) = cell_of(pos);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let Some(neighbors) = grid.get(&(cx + dx, cy + dy)) else {
                        continue;
                    };
                    for &j in neighbors {
                        if i >= j {
                            continue;
                        }
                        let f = repulsion_force(pos, positions[j]);
                        accel[i].0 += f.0;
                        accel[i].1 += f.1;
                        accel[j].0 -= f.0;
                        accel[j].1 -= f.1;
                    }
                }
            }
        }

        // Per-cluster cohesion: pull members toward their centroid (D3). The
        // centroid is recomputed from live positions each iteration, so the
        // force is deterministic and follows the cluster as it moves.
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
                    accel[i].0 += (cx - positions[i].0) * COHESION_K;
                    accel[i].1 += (cy - positions[i].1) * COHESION_K;
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

        if kinetic < ENERGY_THRESHOLD {
            break;
        }
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

fn cell_of(p: (f32, f32)) -> (i32, i32) {
    (
        (p.0 / REPULSION_RADIUS).floor() as i32,
        (p.1 / REPULSION_RADIUS).floor() as i32,
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
    use crate::graph::{Cluster, GraphEdge, GraphNode};
    use crate::latency::CostModel;
    use crate::patch::Patch;
    use std::path::Path;

    fn node(name: &str, section_index: usize) -> GraphNode {
        GraphNode {
            id: (name.to_string(), 0),
            circuit: name.to_string(),
            instance_index: 0,
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
        Graph::build_from_patch(&patch, &clusters, &CostModel::default())
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
        let positions = solve(&graph, &[], DEFAULT_TENSION);
        assert_eq!(positions.len(), 1);
        assert_finite_and_bounded(&positions);
    }

    #[test]
    fn disconnected_graph_produces_finite_positions() {
        // Three independent components with no connecting edges.
        let graph = make_graph(6, &[(0, 1), (2, 3), (4, 5)]);
        let positions = solve(&graph, &[], DEFAULT_TENSION);
        assert_eq!(positions.len(), 6);
        assert_finite_and_bounded(&positions);
    }

    #[test]
    fn cyclic_graph_produces_finite_positions() {
        // A closed loop; topological depth is bounded by the solver.
        let graph = make_graph(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let positions = solve(&graph, &[], DEFAULT_TENSION);
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
        let positions = solve(&graph, &[], DEFAULT_TENSION);
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
        let a = solve(&graph, &[], DEFAULT_TENSION);
        let b = solve(&graph, &[], DEFAULT_TENSION);
        assert_eq!(a, b);
    }

    #[test]
    fn freeze_stability_repeated_queries_are_identical() {
        // After a solve, re-solving the unchanged graph yields identical
        // frozen positions (no drift across queries).
        let graph = make_graph(40, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 0)]);
        let first = solve(&graph, &[], DEFAULT_TENSION);
        let second = solve(&graph, &[], DEFAULT_TENSION);
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
        let low = solve(&graph, &[], TENSION_MIN);
        let low_again = solve(&graph, &[], TENSION_MIN);
        assert_eq!(low, low_again, "same tension must reproduce the layout");
        let high = solve(&graph, &[], TENSION_MAX);
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
            assert_finite_and_bounded(&solve(&graph, &[], t));
            t += TENSION_STEP;
        }
    }

    #[test]
    fn local_resettle_leaves_distant_nodes_unmoved() {
        let graph = make_graph(
            12,
            &[(0, 1), (1, 2), (2, 3), (4, 5), (6, 7), (8, 9), (10, 11)],
        );
        let mut positions = solve(&graph, &[], DEFAULT_TENSION);

        // Teleport node 0 far away; distant nodes must stay put.
        let before = positions.clone();
        positions[0] = (10000.0, 10000.0);
        let found = local_resettle(
            &graph,
            &mut positions,
            &(String::from("n0"), 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            DEFAULT_TENSION,
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
        let mut positions = solve(&graph, &[], DEFAULT_TENSION);
        let before = positions.clone();
        let found = local_resettle(
            &graph,
            &mut positions,
            &(String::from("nope"), 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            DEFAULT_TENSION,
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
                    source: (String::from("n0"), 0),
                    sink: (String::from("n1"), 0),
                },
                GraphEdge {
                    cable: "_C".to_string(),
                    source: (String::from("n1"), 0),
                    sink: (String::from("n2"), 0),
                },
            ],
            clusters,
            validation: vec![],
            ..Default::default()
        };
        let positions = solve(&graph, &[], DEFAULT_TENSION);
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
                    source: (String::from("A"), 0),
                    sink: (String::from("D"), 0),
                },
                GraphEdge {
                    cable: "_Y".to_string(),
                    source: (String::from("B"), 0),
                    sink: (String::from("C"), 0),
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
            DEFAULT_TENSION,
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
            DEFAULT_TENSION,
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
        assert_finite_and_bounded(&solve(&graph, &[], DEFAULT_TENSION));
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
        assert_finite_and_bounded(&solve(&graph, &[], DEFAULT_TENSION));
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
        let baseline = solve(&graph, &[], DEFAULT_TENSION);
        assert_finite_and_bounded(&baseline);
        for _ in 0..3 {
            assert_eq!(
                solve(&graph, &[], DEFAULT_TENSION),
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
        let a = solve(&graph, &[], DEFAULT_TENSION);
        let b = solve(&graph, &[], DEFAULT_TENSION);
        assert_eq!(a, b);
        assert_finite_and_bounded(&a);
    }

    #[test]
    fn local_resettle_is_deterministic_for_same_move() {
        // Determinism extends to the damped re-settle: the same move from the
        // same frozen layout must yield the same result (D9).
        let graph = make_graph(20, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0)]);
        let baseline = solve(&graph, &[], DEFAULT_TENSION);

        let mut a = baseline.clone();
        a[0] = (5000.0, 5000.0);
        local_resettle(
            &graph,
            &mut a,
            &(String::from("n0"), 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            DEFAULT_TENSION,
        );

        let mut b = baseline.clone();
        b[0] = (5000.0, 5000.0);
        local_resettle(
            &graph,
            &mut b,
            &(String::from("n0"), 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            DEFAULT_TENSION,
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

        let mut positions = solve(&graph, &[], DEFAULT_TENSION);
        let resettle_count = resettle_iteration_count(
            &graph,
            &mut positions,
            &(String::from("n0"), 0),
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
        let baseline = solve(&graph, &[], DEFAULT_TENSION);
        let mut positions = baseline.clone();
        positions[0] = (10000.0, 10000.0);
        let found = local_resettle(
            &graph,
            &mut positions,
            &(String::from("n0"), 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[],
            DEFAULT_TENSION,
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

    #[test]
    fn filtered_compact_solve_is_finite_distinct_and_converges() {
        // FULL vs FILTERED: filtered_positions = solve_filtered(filtered_graph)
        // finite, distinct from FULL subset, compact convergence.
        let patch = Patch::from_ini_file(std::path::Path::new(
            "fixtures/modifier_switch_passthrough.ini",
        ))
        .unwrap();
        let clusters: Vec<Cluster> = patch
            .banner_groups
            .iter()
            .map(|g| Cluster {
                title: g.banner.clone().unwrap_or_default(),
                section_range: g.section_range.clone(),
            })
            .collect();
        let full =
            Graph::build_from_patch(&patch, &clusters, &crate::latency::CostModel::default());
        let vars = patch.hw_token_to_vars("B1.1");
        let sub = patch.influence_subtree(&vars);
        let filtered = full.filtered_influence(&sub);
        assert!(!filtered.nodes.is_empty());
        assert!(filtered.nodes.len() < full.nodes.len());
        let full_pos = solve(&full, &[], DEFAULT_TENSION);
        let filt_pos = solve_filtered(&filtered, &[], DEFAULT_TENSION);
        assert_eq!(filt_pos.len(), filtered.nodes.len());
        assert_finite_and_bounded(&filt_pos);
        assert_finite_and_bounded(&full_pos);
        // deterministic: second filtered solve identical
        assert_eq!(filt_pos, solve_filtered(&filtered, &[], DEFAULT_TENSION));
        // distinct from FULL subset: at least one node moved vs its FULL position
        let full_index: HashMap<&NodeId, usize> = full
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (&n.id, i))
            .collect();
        let filt_index: HashMap<&NodeId, usize> = filtered
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (&n.id, i))
            .collect();
        let eps = 1e-3f32;
        let mut any_distinct = false;
        for id in filtered.nodes.iter().map(|n| &n.id) {
            let fi = filt_index[id];
            let gi = full_index[id];
            let (fx, fy) = filt_pos[fi];
            let (gx, gy) = full_pos[gi];
            if (fx - gx).abs() > eps || (fy - gy).abs() > eps {
                any_distinct = true;
                break;
            }
        }
        assert!(
            any_distinct,
            "filtered compact solve must differ from FULL subset projection"
        );
        // compact convergence: bounding box of filtered is within cap and not exploding
        let bbox = |ps: &[(f32, f32)]| {
            let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
            let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
            for (x, y) in ps {
                min_x = min_x.min(*x);
                max_x = max_x.max(*x);
                min_y = min_y.min(*y);
                max_y = max_y.max(*y);
            }
            ((max_x - min_x), (max_y - min_y))
        };
        let (fw, fh) = bbox(&filt_pos);
        assert!(fw.is_finite() && fh.is_finite());
        assert!(fw < 1e6 && fh < 1e6);
        // iteration count within cap
        let filt_iters = solve_iteration_count(&filtered);
        assert!(filt_iters <= SOLVE_ITERATIONS);
        assert!(filt_iters > 0);
    }

    #[test]
    fn filtered_solve_on_empty_is_empty_and_deterministic() {
        let empty = Graph::default();
        let a = solve_filtered(&empty, &[], DEFAULT_TENSION);
        let b = solve_filtered(&empty, &[], DEFAULT_TENSION);
        assert!(a.is_empty());
        assert_eq!(a, b);
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
        let positions = solve(&graph, &[], DEFAULT_TENSION);
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
        let positions = solve(&graph, &[], DEFAULT_TENSION);
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
        let positions = solve(&graph, &[], DEFAULT_TENSION);
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
        let positions = solve(&graph, &[0, 3], DEFAULT_TENSION);
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
        let baseline = solve(&graph, &[], DEFAULT_TENSION);

        // Case A: the moved node itself is pinned at its drop position.
        let mut a = baseline.clone();
        let drop = (baseline[0].0 + 30.0, baseline[0].1 + 10.0);
        a[0] = drop;
        let found = local_resettle(
            &graph,
            &mut a,
            &(String::from("n0"), 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[0],
            DEFAULT_TENSION,
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
            &(String::from("n0"), 0),
            LOCAL_RADIUS,
            LOCAL_ITERATIONS,
            &[1],
            DEFAULT_TENSION,
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
        let free = solve(&graph, &[], DEFAULT_TENSION);
        assert_finite_and_bounded(&free);
        assert_eq!(
            solve(&graph, &[], DEFAULT_TENSION),
            free,
            "&[] must be deterministic"
        );
        assert_eq!(
            solve(&graph, &[999, 1000, usize::MAX], DEFAULT_TENSION),
            free,
            "out-of-range pins must be ignored"
        );
    }

    #[test]
    fn real_patch_solve_settles_quickly() {
        // A real patch (arpeggio1.ini) must stay within the bounded refinement
        // budget: the layered seed is the primary arrangement (design gv5), so
        // the relaxation is a short refinement, not a full energy convergence.
        // The energy-threshold early exit still fires when the graph settles
        // sooner (e.g. cable_banner_combos.ini converges well under the cap).
        let graph = graph_from_fixture("fixtures/arpeggio1.ini");
        assert!(graph.nodes.len() >= 10, "fixture must be non-trivial");
        let count = solve_iteration_count(&graph);
        assert!(
            count <= SOLVE_ITERATIONS,
            "real patch solve took {count} iterations (budget {SOLVE_ITERATIONS})"
        );
        assert!(count > 0);
        assert_finite_and_bounded(&solve(&graph, &[], DEFAULT_TENSION));
    }

    #[test]
    fn real_patch_local_resettle_settles_quickly() {
        // Dragging a node on a real patch re-settles the local neighborhood
        // within the local budget, so the interactive drag stays bounded and
        // responsive. Asserted via the iteration count, not wall-clock timing:
        // the re-settle is capped at LOCAL_ITERATIONS and deterministic.
        let graph = graph_from_fixture("fixtures/arpeggio1.ini");
        let mut positions = solve(&graph, &[], DEFAULT_TENSION);
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
}
