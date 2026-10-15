//! TEMPORARY PROBE (force-vs-column node-body overlap measurement).
//!
//! Quantifies node-body overlap in the graph arrangements so the overlap bug
//! has hard numbers before any fix. Read-only w.r.t. production code: every
//! number here comes from the public `layout` API plus a local copy of the
//! (private) `separate_overlaps` predicate.
//!
//! Run with:
//! ```text
//! cargo test --test force_overlap_probe -- --nocapture
//! ```
//!
//! Keep-or-delete: the column-mode assertions are real regression cover for
//! fixture-sized graphs, so this file is intended to stay (see the report).

use std::path::Path;

use droid_tui::config::LayoutOrdering;
use droid_tui::graph::{Cluster, Graph, GraphOptions};
use droid_tui::latency::CostModel;
use droid_tui::layout::{
    estimated_widths, local_resettle, node_world_sizes, solve, solve_columns, solve_columns_pinned,
    DEFAULT_TENSION, LOCAL_ITERATIONS, LOCAL_RADIUS,
};
use droid_tui::patch::Patch;

/// Mirrors the private `layout::REPULSION_RADIUS` (src/layout.rs:60). Verified
/// from source, not re-derived: the point-repulsion force is exactly zero for
/// centre distances above this value.
const REPULSION_RADIUS: f32 = 120.0;
/// Mirrors the private `layout::SPRING_REST` (src/layout.rs:38).
const SPRING_REST: f32 = 80.0;

/// Mirrors the private `layout::SECTION_SEPARATION_PASSES` (src/layout.rs:95)
/// and `SECTION_SEPARATION_EPS` (src/layout.rs:98).
const SEPARATION_PASSES: usize = 8;
const SEPARATION_EPS: f32 = 1.0;

const FIXTURES: &[&str] = &[
    "fixtures/own_scale.ini",
    "fixtures/droid_mpfs5melody2.ini",
    "fixtures/cable_banner_combos.ini",
    "fixtures/graph_latency_chain.ini",
    "fixtures/own_two_ctrl.ini",
    "fixtures/graph_dependency_walk.ini",
];

/// Measured force-mode overlapping-pair counts (2026-10-15, default tension and
/// cost model). Used only as an upper-bound ratchet: see the assertion in
/// [`force_and_column_overlap_probe`].
const FORCE_PAIR_BUDGET: &[(&str, usize)] = &[
    ("fixtures/own_scale.ini", 224),
    ("fixtures/cable_banner_combos.ini", 7),
    ("fixtures/graph_latency_chain.ini", 6),
    ("fixtures/own_two_ctrl.ini", 4),
    ("fixtures/graph_dependency_walk.ini", 33),
];

/// Build the signal-flow graph for a fixture, mirroring `App::open_graph` and
/// `layout::tests::graph_from_fixture`.
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

#[derive(Debug, Default, Clone, Copy)]
struct OverlapStats {
    /// Pairs whose axis-aligned bodies intersect (penetration > 0).
    pairs: usize,
    /// Worst penetration over all pairs (≤ 0 means fully clear).
    worst: f32,
    /// Mean penetration over the overlapping pairs only.
    mean: f32,
    /// Overlapping pairs whose required x-separation exceeds REPULSION_RADIUS,
    /// i.e. point repulsion can never push them apart horizontally.
    pairs_needing_more_than_cutoff: usize,
    /// Overlapping pairs whose centre distance already exceeds the repulsion
    /// radius, so the point repulsion exerts exactly zero force on them.
    pairs_beyond_repulsion: usize,
}

impl OverlapStats {
    fn line(&self, label: &str) -> String {
        if self.pairs == 0 {
            format!("{label}: pairs=0 worst={:.3} (clear)", self.worst)
        } else {
            format!(
                "{label}: pairs={} worst={:.3} mean={:.3} \
                 need_x_gt_120={} centre_dist_gt_120={}",
                self.pairs,
                self.worst,
                self.mean,
                self.pairs_needing_more_than_cutoff,
                self.pairs_beyond_repulsion
            )
        }
    }
}

/// Axis-aligned body overlap over `positions` — the same predicate as
/// `layout::separate_overlaps` / `layout::tests::worst_penetration`.
fn overlap_stats(sizes: &[(f32, f32)], positions: &[(f32, f32)]) -> OverlapStats {
    let mut stats = OverlapStats {
        worst: f32::MIN,
        ..Default::default()
    };
    for a in 0..positions.len() {
        for b in a + 1..positions.len() {
            let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
            let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
            let dx = (positions[a].0 - positions[b].0).abs();
            let dy = (positions[a].1 - positions[b].1).abs();
            let pen = (need_x - dx).min(need_y - dy);
            stats.worst = stats.worst.max(pen);
            if pen > 0.0 {
                stats.pairs += 1;
                stats.mean += pen;
                if need_x > REPULSION_RADIUS {
                    stats.pairs_needing_more_than_cutoff += 1;
                }
                if (dx * dx + dy * dy).sqrt() > REPULSION_RADIUS {
                    stats.pairs_beyond_repulsion += 1;
                }
            }
        }
    }
    if stats.pairs > 0 {
        stats.mean /= stats.pairs as f32;
    }
    stats
}

/// Local copy of the private `layout::separate_overlaps` (src/layout.rs:346):
/// every node movable, cheapest-axis escape, bounded passes.
/// Returns the number of still-overlapping pairs after the last pass.
fn separate_overlaps_copy(
    positions: &mut [(f32, f32)],
    sizes: &[(f32, f32)],
    passes: usize,
) -> usize {
    let n = positions.len();
    let mut overlapping = 0;
    for _ in 0..passes {
        overlapping = 0;
        for i in 0..n {
            for j in i + 1..n {
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
                if pen_x <= pen_y {
                    let push = (pen_x + SEPARATION_EPS) * 0.5;
                    let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                    positions[i].0 -= sign * push * 0.5;
                    positions[j].0 += sign * push * 0.5;
                } else {
                    let push = (pen_y + SEPARATION_EPS) * 0.5;
                    let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                    positions[i].1 -= sign * push * 0.5;
                    positions[j].1 += sign * push * 0.5;
                }
            }
        }
        if overlapping == 0 {
            break;
        }
    }
    overlapping
}

/// Same pass as [`separate_overlaps_copy`], but pairs are visited bottom-up in
/// y instead of in node-index order: does the sweep order, rather than the pass
/// count, decide whether the pass converges?
fn separate_overlaps_y_sorted_copy(
    positions: &mut [(f32, f32)],
    sizes: &[(f32, f32)],
    passes: usize,
) -> usize {
    let n = positions.len();
    let mut overlapping = 0;
    for _ in 0..passes {
        overlapping = 0;
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| positions[a].1.partial_cmp(&positions[b].1).unwrap());
        for oi in 0..n {
            for oj in oi + 1..n {
                let i = order[oi];
                let j = order[oj];
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
                if pen_x <= pen_y {
                    let push = (pen_x + SEPARATION_EPS) * 0.5;
                    let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                    positions[i].0 -= sign * push * 0.5;
                    positions[j].0 += sign * push * 0.5;
                } else {
                    let push = (pen_y + SEPARATION_EPS) * 0.5;
                    let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                    positions[i].1 -= sign * push * 0.5;
                    positions[j].1 += sign * push * 0.5;
                }
            }
        }
        if overlapping == 0 {
            break;
        }
    }
    overlapping
}

/// Mean/max Euclidean displacement between two position sets.
fn displacement(before: &[(f32, f32)], after: &[(f32, f32)]) -> (f32, f32) {
    let mut max = 0.0f32;
    let mut sum = 0.0f32;
    for i in 0..after.len() {
        let d = ((after[i].0 - before[i].0).powi(2) + (after[i].1 - before[i].1).powi(2)).sqrt();
        max = max.max(d);
        sum += d;
    }
    (max, sum / after.len().max(1) as f32)
}

/// First overlapping pair `(a, b)` in the layout, worst penetration first.
fn worst_pair(sizes: &[(f32, f32)], positions: &[(f32, f32)]) -> Option<(usize, usize)> {
    let mut best: Option<(f32, usize, usize)> = None;
    for a in 0..positions.len() {
        for b in a + 1..positions.len() {
            let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
            let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
            let dx = (positions[a].0 - positions[b].0).abs();
            let dy = (positions[a].1 - positions[b].1).abs();
            let pen = (need_x - dx).min(need_y - dy);
            if pen > 0.0 && best.map(|(p, _, _)| pen > p).unwrap_or(true) {
                best = Some((pen, a, b));
            }
        }
    }
    best.map(|(_, a, b)| (a, b))
}

/// Shape of the overlapping pairs: are they vertical stack mates in one x-layer
/// (same seed layer), or edge-connected (springs), or spread out?
#[derive(Debug, Default, Clone, Copy)]
struct OverlapShape {
    edge_connected: usize,
    mean_need_x: f32,
    mean_dx: f32,
    mean_dy: f32,
    same_x_layer: usize,
}

fn overlap_shape(graph: &Graph, sizes: &[(f32, f32)], positions: &[(f32, f32)]) -> OverlapShape {
    let index: std::collections::HashMap<_, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| (&nd.id, i))
        .collect();
    let mut shape = OverlapShape::default();
    let mut overlapping = 0usize;
    for a in 0..sizes.len() {
        for b in a + 1..sizes.len() {
            let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
            let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
            let dx = (positions[a].0 - positions[b].0).abs();
            let dy = (positions[a].1 - positions[b].1).abs();
            if (need_x - dx).min(need_y - dy) <= 0.0 {
                continue;
            }
            overlapping += 1;
            shape.mean_need_x += need_x;
            shape.mean_dx += dx;
            shape.mean_dy += dy;
            if dx < 1.0 {
                shape.same_x_layer += 1;
            }
            if graph.edges.iter().any(|e| {
                let (Some(&s), Some(&t)) = (index.get(&e.source), index.get(&e.sink)) else {
                    return false;
                };
                (s == a && t == b) || (s == b && t == a)
            }) {
                shape.edge_connected += 1;
            }
        }
    }
    if overlapping > 0 {
        let d = overlapping as f32;
        shape.mean_need_x /= d;
        shape.mean_dx /= d;
        shape.mean_dy /= d;
    }
    shape
}

#[test]
fn force_and_column_overlap_probe() {
    println!("\n=== FIXTURE OVERLAP PROBE (world bodies, NODE_WORLD_H=80) ===");
    let mut force_rows: Vec<(String, usize, OverlapStats)> = Vec::new();
    for name in FIXTURES {
        if !Path::new(name).exists() {
            println!("\n{name}: SKIPPED (fixture not present)");
            continue;
        }
        let graph = graph_from_fixture(name);
        let sizes = node_world_sizes(&graph);
        let widths = estimated_widths(&graph);
        let max_w = widths.iter().cloned().fold(0.0f32, f32::max);
        let n = graph.nodes.len();
        let e = graph.edges.len();

        // Pairs whose required x-separation alone exceeds the repulsion cutoff,
        // and pairs whose required x-separation exceeds the spring rest length.
        let mut pairs_over_cutoff = 0usize;
        let mut pairs_over_rest = 0usize;
        for a in 0..n {
            for b in a + 1..n {
                let need_x = 0.5 * (widths[a] + widths[b]);
                if need_x > REPULSION_RADIUS {
                    pairs_over_cutoff += 1;
                }
                if need_x > SPRING_REST {
                    pairs_over_rest += 1;
                }
            }
        }
        let nodes_wide = widths.iter().filter(|w| **w > REPULSION_RADIUS).count();
        let total_pairs = n * (n.saturating_sub(1)) / 2;

        let force = solve(&graph, &[], DEFAULT_TENSION);
        let force_stats = overlap_stats(&sizes, &force);
        let col = solve_columns(&graph, &widths, LayoutOrdering::Strict);
        let col_stats = overlap_stats(&sizes, &col);

        println!(
            "\n{name}: nodes={n} edges={e} max_w={max_w:.1} nodes_w_gt_120={nodes_wide} \
             pairs_need_x_gt_120={pairs_over_cutoff}/{total_pairs} \
             pairs_need_x_gt_rest80={pairs_over_rest}/{total_pairs}"
        );
        println!("  {}", force_stats.line("force(solve)"));
        println!("  {}", col_stats.line("columns(strict)"));
        let shape = overlap_shape(&graph, &sizes, &force);
        println!(
            "  (shape) edge_connected={}/{} mean_need_x={:.1} mean_|dx|={:.1} mean_|dy|={:.1} \
             same_x_layer={}",
            shape.edge_connected,
            force_stats.pairs,
            shape.mean_need_x,
            shape.mean_dx,
            shape.mean_dy,
            shape.same_x_layer
        );

        // Column mode must be clean — that is the documented contract.
        assert_eq!(
            col_stats.pairs, 0,
            "{name}: column mode must not overlap (worst {:.3})",
            col_stats.worst
        );
        assert!(force_stats.worst.is_finite());

        // Ratchet on the known force-mode defect (measured 2026-10-15): the
        // count may only improve. The correct fix drives these to 0; the bound
        // exists to catch a regression, not to bless the current overlap.
        if let Some(&(_, budget)) = FORCE_PAIR_BUDGET.iter().find(|(f, _)| *f == *name) {
            assert!(
                force_stats.pairs <= budget,
                "{name}: force-mode overlap got worse: {} pairs > recorded {budget}",
                force_stats.pairs
            );
        }

        force_rows.push((name.to_string(), n, force_stats));
    }

    // ── worst force fixture: scenario deep dive ─────────────────────────────
    force_rows.sort_by(|a, b| {
        b.2.pairs
            .cmp(&a.2.pairs)
            .then(b.2.worst.partial_cmp(&a.2.worst).unwrap())
    });
    let Some((worst_name, _, worst_stats)) = force_rows.first().cloned() else {
        println!("\nno fixtures present; nothing to deep-dive");
        return;
    };
    println!(
        "\n=== WORST FORCE FIXTURE: {worst_name} ({} overlapping pairs, worst {:.3}) ===",
        worst_stats.pairs, worst_stats.worst
    );

    let graph = graph_from_fixture(&worst_name);
    let widths = estimated_widths(&graph);
    let sizes = node_world_sizes(&graph);
    let force = solve(&graph, &[], DEFAULT_TENSION);

    // ── characterise the overlapping pairs ─────────────────────────────────
    let shape = overlap_shape(&graph, &sizes, &force);
    println!(
        "  (char) overlapping pairs: edge_connected={}/{} mean_need_x={:.1} mean_|dx|={:.1} \
         mean_|dy|={:.1} same_x_layer={}",
        shape.edge_connected,
        worst_stats.pairs,
        shape.mean_need_x,
        shape.mean_dx,
        shape.mean_dy,
        shape.same_x_layer
    );

    // (2a) bounded post-pass alone on the force result — current 8 passes plus
    // longer budgets to see whether the pass converges at all.
    for passes in [1usize, 2, 4, SEPARATION_PASSES, 16, 32, 64] {
        let mut after = force.clone();
        let remaining = separate_overlaps_copy(&mut after, &sizes, passes);
        let after_stats = overlap_stats(&sizes, &after);
        let (disp_max, disp_mean) = displacement(&force, &after);
        let checksum: f32 = after.iter().map(|p| p.0 + 3.0 * p.1).sum();
        println!(
            "  (2a) post-pass passes={passes:>2}: pairs={} (returned {remaining}) worst={:.3} \
             max_disp={disp_max:.2} mean_disp={disp_mean:.2} checksum={checksum:.3}",
            after_stats.pairs, after_stats.worst
        );
    }
    // Same pass, but pairs visited in y order instead of node-index order: does
    // the sweep order, rather than the pass count, decide convergence?
    {
        let mut sorted: Vec<usize> = (0..force.len()).collect();
        sorted.sort_by(|&a, &b| force[a].1.partial_cmp(&force[b].1).unwrap());
        let differing = sorted
            .iter()
            .enumerate()
            .filter(|(rank, &node)| rank != &node)
            .count();
        println!(
            "  (2a-Y order check) y-sorted order differs from index order at {differing}/{} positions",
            force.len()
        );
    }
    for passes in [SEPARATION_PASSES, 64] {
        let mut after = force.clone();
        separate_overlaps_y_sorted_copy(&mut after, &sizes, passes);
        let after_stats = overlap_stats(&sizes, &after);
        let (disp_max, disp_mean) = displacement(&force, &after);
        let checksum: f32 = after.iter().map(|p| p.0 + 3.0 * p.1).sum();
        println!(
            "  (2a-Y) post-pass y-ordered passes={passes:>2}: pairs={} worst={:.3} \
             max_disp={disp_max:.2} mean_disp={disp_mean:.2} checksum={checksum:.3}",
            after_stats.pairs, after_stats.worst
        );
    }

    // (2b) force positions carried in as pins into the column arrangement.
    let all_pins: Vec<(usize, (f32, f32))> = force.iter().cloned().enumerate().collect();
    let pinned_all = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &all_pins);
    let pinned_all_stats = overlap_stats(&sizes, &pinned_all);
    println!(
        "  (2b-ALL) every node pinned at its force position: {}",
        pinned_all_stats.line("columns")
    );
    assert_eq!(
        pinned_all_stats.pairs, worst_stats.pairs,
        "pinning every node must reproduce the carried layout exactly"
    );

    // (2b-PAIR) only the worst overlapping pair pinned, everything else
    // column-arranged: does a carried pin drag its overlap into column mode?
    if let Some((wa, wb)) = worst_pair(&sizes, &force) {
        let pins = vec![(wa, force[wa]), (wb, force[wb])];
        let out = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &pins);
        println!(
            "  (2b-PAIR) only the worst overlapping pair ({wa},{wb}) pinned: {}",
            overlap_stats(&sizes, &out).line("columns")
        );

        // Keep a pinned pair's exact separation: report the carried overlap.
        let need_x = 0.5 * (sizes[wa].0 + sizes[wb].0);
        let dx = (force[wa].0 - force[wb].0).abs();
        let dy = (force[wa].1 - force[wb].1).abs();
        println!(
            "  (2b-PAIR) carried pair separation: need_x={need_x:.1} |dx|={dx:.1} |dy|={dy:.1} \
             (pins honored exactly, so the pair stays overlapping)"
        );
    }

    // (2b-SWEEP) single off-grid pin sweep: how often does pinning one node at
    // its carried (non-grid) force position break the column arrangement?
    let sweep_indices: Vec<usize> = if force.len() <= 100 {
        (0..force.len()).collect()
    } else {
        (0..force.len())
            .step_by((force.len() / 60).max(1))
            .collect()
    };
    let mut pins_causing_overlap = 0usize;
    let mut worst_single = f32::MIN;
    let mut worst_single_node = None;
    for &i in &sweep_indices {
        let out = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[(i, force[i])]);
        let s = overlap_stats(&sizes, &out);
        if s.pairs > 0 {
            pins_causing_overlap += 1;
            if s.worst > worst_single {
                worst_single = s.worst;
                worst_single_node = Some(i);
            }
        }
    }
    if worst_single_node.is_none() {
        worst_single = 0.0;
    }
    println!(
        "  (2b-SWEEP) single off-grid pin over {} sampled nodes: {pins_causing_overlap} broke \
             the no-overlap property (worst penetration {:.2}, node {worst_single_node:?})",
        sweep_indices.len(),
        worst_single
    );

    // (2b-OFFGRID) deliberately non-slot-aligned carried pins: a pin at a
    // mid-slot y reserves the *nearest* slot only, so the unpinned node that
    // takes the slot below it shares vertical space with the pinned body.
    for &i in sweep_indices.iter().take(3) {
        let off_grid = (force[i].0, 60.0_f32 + 240.0 * (i as f32 / 97.0).floor());
        let out = solve_columns_pinned(&graph, &widths, LayoutOrdering::Strict, &[(i, off_grid)]);
        println!(
            "  (2b-OFFGRID) node {i} pinned at y={:.0} (mid-slot): {}",
            off_grid.1,
            overlap_stats(&sizes, &out).line("columns")
        );
    }

    // (2c) local re-settle after moving one node onto its worst neighbour.
    let Some((wa, wb)) = worst_pair(&sizes, &force) else {
        println!("  (2c) no overlapping pair to move onto");
        return;
    };
    let mut dragged = force.clone();
    let before = dragged.clone();
    dragged[wb] = dragged[wa]; // drop node wb exactly onto node wa
    let moved_id = graph.nodes[wb].id.clone();
    let ok = local_resettle(
        &graph,
        &mut dragged,
        &moved_id,
        LOCAL_RADIUS,
        LOCAL_ITERATIONS,
        &[],
        DEFAULT_TENSION,
    );
    let resettle_stats = overlap_stats(&sizes, &dragged);
    let (disp_max, disp_mean) = displacement(&before, &dragged[..before.len().min(dragged.len())]);
    println!(
        "  (2c) local_resettle (node {wb} dropped onto node {wa} `{}`, ok={ok}, \
         radius={LOCAL_RADIUS}, iters={LOCAL_ITERATIONS}): {} max_disp={disp_max:.2} mean_disp={disp_mean:.2}",
        graph.nodes[wa].id.name(),
        resettle_stats.line("after_resettle")
    );

    // (3) does the 120 repulsion cutoff explain the overlap?
    println!(
        "  (3) cutoff diagnosis: {} of {} force overlaps need_x > 120, {} of {} have \
         centre distance > 120 (repulsion identically zero there)",
        worst_stats.pairs_needing_more_than_cutoff,
        worst_stats.pairs,
        worst_stats.pairs_beyond_repulsion,
        worst_stats.pairs
    );
    let max_w = widths.iter().cloned().fold(0.0f32, f32::max);
    let nodes_wide = widths.iter().filter(|w| **w > REPULSION_RADIUS).count();
    let pairs_over_rest = {
        let mut c = 0usize;
        for a in 0..widths.len() {
            for b in a + 1..widths.len() {
                if 0.5 * (widths[a] + widths[b]) > SPRING_REST {
                    c += 1;
                }
            }
        }
        c
    };
    let total = widths.len() * widths.len().saturating_sub(1) / 2;
    println!(
        "  (3) widths: max={max_w:.1} nodes_w_gt_120={nodes_wide}/{} \
         pairs_needing_more_than_spring_rest_80={pairs_over_rest}/{total}",
        widths.len()
    );
}

// ── controlled experiment: is cluster cohesion the collapser? ────────────────

/// One source feeding `n_sinks` sinks: the sinks share topological depth 1 and
/// therefore sit in one seed x-layer at `VERTICAL_SPACING` (120) pitch. With
/// `clustered == false` nothing pulls them together in y; with `clustered ==
/// true` a single banner group spans every node, so `COHESION_K` pulls them
/// toward the group centroid.
fn fanout_graph(n_sinks: usize, clustered: bool) -> Graph {
    let mut nodes = vec![droid_tui::graph::GraphNode {
        id: droid_tui::patch::NodeId::circuit("src", 0),
        kind: droid_tui::graph::NodeKind::Circuit,
        circuit: "src".to_string(),
        instance_index: 0,
        section_index: 0,
    }];
    let mut edges = Vec::new();
    for i in 0..n_sinks {
        nodes.push(droid_tui::graph::GraphNode {
            id: droid_tui::patch::NodeId::circuit(&format!("sink{i}"), 0),
            kind: droid_tui::graph::NodeKind::Circuit,
            circuit: format!("sink{i}"),
            instance_index: 0,
            section_index: i + 1,
        });
        edges.push(droid_tui::graph::GraphEdge {
            cable: "_C".to_string(),
            source: nodes[0].id.clone(),
            sink: nodes[i + 1].id.clone(),
        });
    }
    let clusters = if clustered {
        vec![Cluster {
            title: "grp".to_string(),
            section_range: 0..n_sinks + 1,
        }]
    } else {
        Vec::new()
    };
    Graph {
        nodes,
        edges,
        clusters,
        ..Default::default()
    }
}

/// Vertical pitch of the seed layer (min |dy| between consecutive stack mates),
/// and the mean vertical spread of the layer about its own centroid.
fn stack_pitch(positions: &[(f32, f32)]) -> (f32, f32) {
    let mut ys: Vec<f32> = positions.iter().map(|p| p.1).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min_pitch = ys
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(f32::INFINITY, f32::min);
    let centroid = ys.iter().sum::<f32>() / ys.len() as f32;
    let spread = ys.iter().map(|y| (y - centroid).abs()).sum::<f32>() / ys.len() as f32;
    (min_pitch, spread)
}

#[test]
fn cohesion_collapses_a_same_layer_stack() {
    println!("\n=== COHESION CONTROLLED EXPERIMENT (1 source -> N sinks, one x-layer) ===");
    for n_sinks in [8usize, 16, 40] {
        for clustered in [false, true] {
            let graph = fanout_graph(n_sinks, clustered);
            let sizes = node_world_sizes(&graph);
            let positions = solve(&graph, &[], DEFAULT_TENSION);
            let stats = overlap_stats(&sizes, &positions);
            let (min_pitch, spread) = stack_pitch(&positions);
            println!(
                "  sinks={n_sinks:>2} clustered={clustered:<5}: {} | min_stack_pitch={min_pitch:.1} \
                 mean_|y-centroid|={spread:.1}",
                stats.line("force"),
            );
        }
    }
    println!(
        "  (seed contract: VERTICAL_SPACING=120 pitch is overlap-free for NODE_WORLD_H=80; \
         SEPARATION_* is applied only in Sections mode)"
    );
}
