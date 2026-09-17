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

use crate::config::LayoutOrdering;
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

/// Per-character width of a node title in the size estimator.
const NODE_ESTIMATE_CHAR_WIDTH: f32 = 8.0;
/// Left + right frame padding in the size estimator.
const NODE_ESTIMATE_PADDING: f32 = 16.0;
/// Width reserved per input/output port marker in the size estimator.
const NODE_ESTIMATE_PORT_WIDTH: f32 = 14.0;

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
    // Longest-path topological depth ([`topological_depth`]) with
    // crossing-minimized within-layer order (barycenter sweeps, deterministic).
    let depth = topological_depth(graph);
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
/// Pins are honored exactly; the no-overlap property holds when pinned
/// positions are grid-aligned (as previous column-solve outputs are).
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

/// Estimated rendered width of node `i` (graph-column-layout D3, task 1.3):
/// title characters at [`NODE_ESTIMATE_CHAR_WIDTH`] each, frame padding
/// ([`NODE_ESTIMATE_PADDING`]), plus a fixed marker width per input/output
/// port the node has edges for ([`NODE_ESTIMATE_PORT_WIDTH`]) — the same
/// inputs the renderer uses (title from `instance_title`, port presence from
/// incidence). Non-circuit nodes (controllers/jacks) have no instance suffix,
/// mirroring the renderer.
fn estimate_node_width(graph: &Graph, i: usize) -> f32 {
    let node = &graph.nodes[i];
    let title = if node.kind == NodeKind::Circuit {
        let repeats = graph
            .nodes
            .iter()
            .filter(|o| o.kind == NodeKind::Circuit && o.circuit == node.circuit)
            .count();
        estimated_title(&node.circuit, node.instance_index, Some(repeats))
    } else {
        node.circuit.clone()
    };
    let title_w = title.chars().count() as f32 * NODE_ESTIMATE_CHAR_WIDTH;
    let index = node_index(graph);
    let edges = edge_pairs(graph, &index);
    let has_in = edges.iter().any(|&(_, v)| v == i);
    let has_out = edges.iter().any(|&(u, _)| u == i);
    let ports_w = NODE_ESTIMATE_PORT_WIDTH * (usize::from(has_in) + usize::from(has_out)) as f32;
    title_w + NODE_ESTIMATE_PADDING + ports_w
}

/// Estimated width of every node (graph-column-layout D3, task 1.3): the
/// renderer-input sizes that feed the column placement, so wide nodes widen
/// their column. Deterministic; rendering remains size-authoritative.
pub fn estimated_widths(graph: &Graph) -> Vec<f32> {
    (0..graph.nodes.len())
        .map(|i| estimate_node_width(graph, i))
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
            &NodeId::circuit("n0", 0),
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
            &NodeId::circuit("nope", 0),
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
            &NodeId::circuit("n0", 0),
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
            &NodeId::circuit("n0", 0),
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
        let baseline = solve(&graph, &[], DEFAULT_TENSION);
        let mut positions = baseline.clone();
        positions[0] = (10000.0, 10000.0);
        let found = local_resettle(
            &graph,
            &mut positions,
            &NodeId::circuit("n0", 0),
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
            &NodeId::circuit("n0", 0),
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
            &NodeId::circuit("n0", 0),
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
}
