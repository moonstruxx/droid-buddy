//! Pure forward-loop latency model for the signal-flow graph (design D1).
//!
//! DROID evaluates circuits serially in file order once per loop, so a sink
//! that reads a cable produced by a *later* circuit gets last loop's value.
//! [`forward_latency`] turns that wrap-around into a per-edge latency
//! `L(S→T) = ((t − s) mod N) × AVG` — `s`/`t` are the source/sink file-order
//! positions, `N` is the module count, `AVG` is the producing circuit's
//! RAM-proportional cost. A back-edge (`s > t`) wraps the loop boundary and
//! lands on the red end of the latency ramp.
//!
//! Pure module: no terminal dependency, no I/O, no RNG. Edges are iterated by
//! index and positions resolve through a node-order lookup, so identical input
//! yields byte-identical output.

use std::collections::HashMap;

use crate::graph::{GraphEdge, NodeId};

/// Per-edge latency, parallel to `Graph.edges` (`edge_index` is the index into
/// the edges slice).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeLatency {
    /// Index into the `edges` slice this entry describes.
    pub edge_index: usize,
    /// `L(S→T) = ((t − s) mod N) × AVG`, in loop units. Dividing by `N × AVG`
    /// yields the normalized 0..1 gradient axis used by the renderer's
    /// blue→red ramp.
    pub latency: f32,
    /// `true` when the edge wraps the loop boundary (`s > t`): the sink reads
    /// last loop's value and the edge lands at the red end of the ramp.
    pub is_back_edge: bool,
}

/// Aggregate latency over every edge (empty graph → all zeros).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencySummary {
    /// Mean latency over all edges.
    pub avg: f32,
    /// Largest single-edge latency.
    pub max: f32,
    /// Number of edges with `is_back_edge == true`.
    pub back_edge_count: usize,
}

/// Per-graph latency result (design D2): per-edge latencies parallel to
/// `Graph.edges` plus the aggregate summary. `Graph.latency` is `None` when
/// there is nothing to measure (no nodes or no edges), so a `LatencyData`
/// always describes a non-empty edge list.
#[derive(Debug, Clone, PartialEq)]
pub struct LatencyData {
    /// Per-edge latency, parallel to `Graph.edges` by index.
    pub edges: Vec<EdgeLatency>,
    /// Aggregate over `edges`.
    pub summary: LatencySummary,
}

// `Graph` derives `Eq`, which `LatencyData` must support despite its f32
// fields (`latency`/`avg`/`max`). The values are computed from integer RAM
// sizes, never NaN, so `==` is a genuine equivalence relation here and a
// manual marker impl is sound.
impl Eq for LatencyData {}

/// Compute per-edge forward-loop latency and the aggregate summary.
///
/// - `edges` — the graph's edges, in `Graph.edges` order; each result's
///   `edge_index` mirrors the position in this slice.
/// - `node_positions` — `(node id, section_index)` per node, in `Graph.nodes`
///   order. `N = node_positions.len()` is the module count used by the modulo.
/// - `circuit_avg` — per-`NodeId` processing cost `AVG ∝ ramsize(circuit)`.
///   The producing (source) circuit's cost scales the edge's latency.
pub fn forward_latency(
    edges: &[GraphEdge],
    node_positions: &[(NodeId, usize)],
    circuit_avg: impl Fn(&NodeId) -> f32,
) -> (Vec<EdgeLatency>, LatencySummary) {
    let n = node_positions.len();
    // NodeId → node index, built in node order so lookups are deterministic.
    let mut node_index: HashMap<&NodeId, usize> = HashMap::new();
    for (i, (id, _)) in node_positions.iter().enumerate() {
        node_index.insert(id, i);
    }
    // Fall back to position 0 for an edge endpoint missing from the node list
    // so the model never panics on an inconsistent input.
    let position = |id: &NodeId| -> usize {
        node_index
            .get(id)
            .and_then(|&i| node_positions.get(i))
            .map_or(0, |(_, section_index)| *section_index)
    };

    let mut latencies = Vec::with_capacity(edges.len());
    let mut sum = 0.0f32;
    let mut max = 0.0f32;
    let mut back_edge_count = 0usize;

    for (edge_index, edge) in edges.iter().enumerate() {
        let s = position(&edge.source);
        let t = position(&edge.sink);
        let is_back_edge = s > t;
        // ((t − s) mod N); rem_euclid wraps negatives so a back-edge's distance
        // is measured past the loop boundary. Degenerate n == 0 can only
        // coincide with an empty edge list, but guard the division anyway.
        let distance = if n == 0 {
            0
        } else {
            ((t as isize - s as isize).rem_euclid(n as isize)) as usize
        };
        let latency = distance as f32 * circuit_avg(&edge.source);
        if is_back_edge {
            back_edge_count += 1;
        }
        max = max.max(latency);
        sum += latency;
        latencies.push(EdgeLatency {
            edge_index,
            latency,
            is_back_edge,
        });
    }

    let avg = if edges.is_empty() {
        0.0
    } else {
        sum / edges.len() as f32
    };
    (
        latencies,
        LatencySummary {
            avg,
            max,
            back_edge_count,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(cable: &str, source: (&str, usize), sink: (&str, usize)) -> GraphEdge {
        GraphEdge {
            cable: cable.to_string(),
            source: (source.0.to_string(), source.1),
            sink: (sink.0.to_string(), sink.1),
        }
    }

    fn node(id: (&str, usize), section_index: usize) -> (NodeId, usize) {
        ((id.0.to_string(), id.1), section_index)
    }

    /// Modules `A B C D` in file order (positions 0..3), as in the spec
    /// scenarios.
    fn abcd_nodes() -> Vec<(NodeId, usize)> {
        vec![
            node(("A", 0), 0),
            node(("B", 0), 1),
            node(("C", 0), 2),
            node(("D", 0), 3),
        ]
    }

    #[test]
    fn forward_chain_all_low_and_not_back_edges() {
        let nodes = abcd_nodes();
        let edges = vec![
            edge("_AB", ("A", 0), ("B", 0)),
            edge("_BC", ("B", 0), ("C", 0)),
            edge("_CD", ("C", 0), ("D", 0)),
        ];
        let (latencies, summary) = forward_latency(&edges, &nodes, |_| 1.0);

        assert_eq!(latencies.len(), 3);
        for (i, l) in latencies.iter().enumerate() {
            assert_eq!(l.edge_index, i);
            assert!(!l.is_back_edge, "edge {i} must be forward");
            assert_eq!(l.latency, 1.0, "adjacent edges cost one loop step");
        }
        assert_eq!(summary.avg, 1.0);
        assert_eq!(summary.max, 1.0);
        assert_eq!(summary.back_edge_count, 0);
    }

    #[test]
    fn forward_edge_skipping_modules_scales_with_distance() {
        // Spec scenario: A produces, D consumes (positions 0 and 3, N = 4).
        let nodes = abcd_nodes();
        let edges = vec![edge("_AD", ("A", 0), ("D", 0))];
        let (latencies, summary) = forward_latency(&edges, &nodes, |_| 1.0);

        assert_eq!(latencies.len(), 1);
        let l = &latencies[0];
        assert!(!l.is_back_edge);
        assert_eq!(l.latency, 3.0, "((3 − 0) mod 4) × AVG = 3 × AVG");
        assert_eq!(summary.avg, 3.0);
        assert_eq!(summary.max, 3.0);
        assert_eq!(summary.back_edge_count, 0);
    }

    #[test]
    fn back_edge_wraps_the_loop_boundary() {
        // Spec scenario: D produces, B consumes (positions 3 and 1, N = 4).
        let nodes = abcd_nodes();
        let edges = vec![edge("_DB", ("D", 0), ("B", 0))];
        let (latencies, summary) = forward_latency(&edges, &nodes, |_| 1.0);

        assert_eq!(latencies.len(), 1);
        let l = &latencies[0];
        assert!(l.is_back_edge, "source after sink must wrap the loop");
        assert_eq!(l.latency, 2.0, "((1 − 3) mod 4) × AVG = 2 × AVG");
        assert_eq!(summary.back_edge_count, 1);
        assert_eq!(summary.avg, 2.0);
        assert_eq!(summary.max, 2.0);
    }

    #[test]
    fn empty_graph_yields_empty_result_and_zero_summary() {
        let (latencies, summary) = forward_latency(&[], &[], |_| 1.0);
        assert!(latencies.is_empty());
        assert_eq!(summary.avg, 0.0);
        assert_eq!(summary.max, 0.0);
        assert_eq!(summary.back_edge_count, 0);
    }

    #[test]
    fn forward_latency_is_deterministic() {
        let nodes = abcd_nodes();
        let edges = vec![
            edge("_AD", ("A", 0), ("D", 0)),
            edge("_DB", ("D", 0), ("B", 0)),
            edge("_BC", ("B", 0), ("C", 0)),
        ];
        let avg = |_: &NodeId| 1.0;
        let (a_lat, a_sum) = forward_latency(&edges, &nodes, avg);
        let (b_lat, b_sum) = forward_latency(&edges, &nodes, avg);
        assert_eq!(a_lat, b_lat, "two runs must produce identical latencies");
        assert_eq!(a_sum, b_sum, "two runs must produce identical summaries");
    }

    #[test]
    fn avg_scales_with_circuit_cost() {
        // AVG ∝ ramsize: a heavier source circuit scales the whole edge.
        let nodes = abcd_nodes();
        let edges = vec![edge("_AD", ("A", 0), ("D", 0))];
        let (latencies, _) = forward_latency(&edges, &nodes, |id| {
            if *id == ("A".to_string(), 0) {
                2.0
            } else {
                1.0
            }
        });
        assert_eq!(latencies[0].latency, 6.0, "3 steps × AVG 2.0");
    }

    #[test]
    fn back_edge_count_totals_all_wrapping_edges() {
        let nodes = abcd_nodes();
        let edges = vec![
            edge("_AB", ("A", 0), ("B", 0)), // forward
            edge("_DB", ("D", 0), ("B", 0)), // back
            edge("_CA", ("C", 0), ("A", 0)), // back
            edge("_BC", ("B", 0), ("C", 0)), // forward
        ];
        let (latencies, summary) = forward_latency(&edges, &nodes, |_| 1.0);
        assert_eq!(summary.back_edge_count, 2);
        assert_eq!(summary.max, 2.0);
        assert_eq!(
            summary.avg,
            (1.0 + 2.0 + 2.0 + 1.0) / 4.0,
            "summary counts all four edges"
        );
        assert_eq!(
            latencies.iter().map(|l| l.edge_index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }
}
