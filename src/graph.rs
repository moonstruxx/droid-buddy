//! Signal-flow graph model: circuits as nodes, virtual cables as directed edges.
//!
//! Pure module (design D5) — no terminal dependency, so it is testable without
//! rendering. `build_from_patch` turns a parsed `Patch` (cable index + sections)
//! plus caller-supplied banner clusters into a graph the renderer can draw and
//! the layout solver can position.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::patch::{InfluenceSubtree, Patch};

/// A node's identity: `(circuit_name, instance_index)`.
///
/// Repeated section names are distinct circuit instances (e.g. two `[copy]`
/// sections), so the section name alone is not a unique key. `instance_index`
/// is the zero-based occurrence order among same-named sections in the file.
pub type NodeId = (String, usize);

/// A circuit (section) rendered as a graph node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: NodeId,
    /// Circuit/section name, e.g. `"clocktool"`.
    pub circuit: String,
    /// Zero-based occurrence index among same-named sections.
    pub instance_index: usize,
    /// Zero-based position of this section in `Patch.sections`; lets a consumer
    /// map a node into a `Cluster`'s `section_range`.
    pub section_index: usize,
}

/// A directed edge from one circuit's cable source to one cable sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    /// Virtual cable name, e.g. `"_PULSARCLOCK"`. Cable color-type inference is
    /// a rendering concern (design D8); the model carries the name only.
    pub cable: String,
    pub source: NodeId,
    pub sink: NodeId,
}

/// A banner-group cluster: a titled range of sections.
///
/// `section_range` indexes into `Patch.sections` (`[start, end)`). Callers pass
/// clusters derived from `Patch.banner_groups` (task 1.2) — a recorded
/// implementation-time decision per design.md Open Questions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    pub title: String,
    pub section_range: Range<usize>,
}

/// Severity of a topology-validation finding (design D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologySeverity {
    /// A cable referenced as a sink but produced by no circuit (dangling).
    Warning,
    /// Multiple circuits producing one cable (`n → 1`), which is invalid.
    Error,
}

/// A topology-validation finding attached to a cable (design D4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyIssue {
    pub cable: String,
    pub severity: TopologySeverity,
    pub message: String,
}

/// The signal-flow graph: circuit nodes, directed cable edges, banner clusters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub clusters: Vec<Cluster>,
    /// Topology-validation results (exactly-one-source, dangling, `n → 1`).
    /// Reserved for task 2.2 (design D4): the slot is always empty today so
    /// validation results can travel with the graph once the pass is added.
    pub validation: Vec<TopologyIssue>,
    /// Highlight sets for FULL graph rendering when an influence is active.
    /// `highlighted_nodes` are `NodeId`s from `InfluenceSubtree::influenced_nodes`;
    /// `highlighted_edges` are cable names from `InfluenceSubtree::influenced_edges`
    /// (edge is highlighted when its cable is in the set). Empty means no
    /// influence is active and the FULL graph renders without dim/highlight
    /// override. Pure, no IO; derived from `crate::patch::InfluenceSubtree` so
    /// there is a single source of truth (patch owns the walk, graph owns the
    /// rendering state).
    #[allow(clippy::type_complexity)]
    pub highlighted_nodes: HashSet<NodeId>,
    pub highlighted_edges: HashSet<String>,
}

impl Graph {
    /// Build a signal-flow graph from a parsed `Patch`.
    ///
    /// - Every section becomes a node; repeated names are distinct instances.
    /// - Each cable in `patch.cable_index` fans its source out to every sink
    ///   reference, producing one directed edge per (cable, sink).
    /// - `clusters` are stored verbatim; callers pass banner groups derived
    ///   from `Patch.banner_groups` (task 1.2).
    ///
    /// Cable attribution is by section *name* (the cable index records names,
    /// not instance indices), so a name shared by several instances resolves
    /// to the first instance. Instance-accurate attribution is left to the
    /// topology-validation pass (task 2.2), which operates on the cable index
    /// entries by name, keeping that convention consistent.
    pub fn build_from_patch(patch: &Patch, clusters: &[Cluster]) -> Graph {
        let nodes = build_nodes(patch);
        let node_by_name = name_to_first_node(&nodes);
        let edges = build_edges(patch, &node_by_name);
        let validation = validate_topology(patch);
        Graph {
            nodes,
            edges,
            clusters: clusters.to_vec(),
            validation,
            highlighted_nodes: HashSet::new(),
            highlighted_edges: HashSet::new(),
        }
    }

    /// Return a clone of this graph with highlight sets derived from `subtree`.
    ///
    /// Used for FULL graph rendering: influenced edges/nodes render with
    /// `graph_edge_highlight` / `graph_node_highlight`, the rest dimmed.
    /// Pure, no IO.
    pub fn with_highlights(&self, subtree: &InfluenceSubtree) -> Graph {
        let mut out = self.clone();
        out.highlighted_nodes = subtree.influenced_nodes.clone();
        out.highlighted_edges = subtree.influenced_edges.clone();
        out
    }

    /// Build the induced subgraph for `subtree` (FILTERED pane).
    ///
    /// - `nodes` = those with `id` in `subtree.influenced_nodes`
    /// - `edges` = those with `cable` in `subtree.influenced_edges` and both
    ///   endpoints in the filtered node set
    /// - `clusters` = banner clusters filtered to ranges intersecting the
    ///   filtered node `section_indices`
    /// - `validation` = filtered to cables in `subtree.influenced_edges`
    ///   (re-runs topology validation on the subgraph; the filtered validation
    ///   never introduces new cables, only narrows the FULL graph's findings)
    /// - `highlighted_*` for the filtered graph is cleared — the pane is
    ///   uniformly highlighted (compact re-solve), so dim/highlight is not
    ///   needed.
    ///
    /// Deterministic: node/edge order is preserved (edges were already
    /// sorted by `(cable, source, sink)` in `build_edges`), clusters keep
    /// caller order. Pure, no IO.
    pub fn filtered_influence(&self, subtree: &InfluenceSubtree) -> Graph {
        let nodes: Vec<GraphNode> = self
            .nodes
            .iter()
            .filter(|n| subtree.influenced_nodes.contains(&n.id))
            .cloned()
            .collect();
        let node_ids: HashSet<NodeId> = nodes.iter().map(|n| n.id.clone()).collect();
        let mut edges: Vec<GraphEdge> = self
            .edges
            .iter()
            .filter(|e| {
                subtree.influenced_edges.contains(&e.cable)
                    && node_ids.contains(&e.source)
                    && node_ids.contains(&e.sink)
            })
            .cloned()
            .collect();
        edges.sort_by(|a, b| (&a.cable, &a.source, &a.sink).cmp(&(&b.cable, &b.source, &b.sink)));
        let clusters: Vec<Cluster> = self
            .clusters
            .iter()
            .filter(|c| {
                nodes
                    .iter()
                    .any(|n| c.section_range.contains(&n.section_index))
            })
            .cloned()
            .collect();
        let validation: Vec<TopologyIssue> = self
            .validation
            .iter()
            .filter(|iss| subtree.influenced_edges.contains(&iss.cable))
            .cloned()
            .collect();
        Graph {
            nodes,
            edges,
            clusters,
            validation,
            highlighted_nodes: HashSet::new(),
            highlighted_edges: HashSet::new(),
        }
    }
}

/// Build one node per section, assigning distinct instance indices to
/// same-named sections in file order.
fn build_nodes(patch: &Patch) -> Vec<GraphNode> {
    let mut instance_counts: HashMap<&str, usize> = HashMap::new();
    patch
        .sections
        .iter()
        .enumerate()
        .map(|(section_index, section)| {
            let count = instance_counts.entry(&section.name).or_insert(0);
            let instance_index = *count;
            *count += 1;
            GraphNode {
                id: (section.name.clone(), instance_index),
                circuit: section.name.clone(),
                instance_index,
                section_index,
            }
        })
        .collect()
}

/// Map each distinct circuit name to its first node (instance 0).
fn name_to_first_node(nodes: &[GraphNode]) -> HashMap<&str, NodeId> {
    let mut map = HashMap::new();
    for node in nodes {
        map.entry(node.circuit.as_str())
            .or_insert_with(|| node.id.clone());
    }
    map
}

/// Build one directed edge per (cable, source, sink) combination.
///
/// A cable only produces edges when it has a resolvable source; a sink name
/// that resolves to no node is skipped rather than panicking.
fn build_edges(patch: &Patch, node_by_name: &HashMap<&str, NodeId>) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    for (cable, entry) in &patch.cable_index {
        for source_name in &entry.sources {
            let Some(source) = node_by_name.get(source_name.as_str()) else {
                continue;
            };
            for (sink_name, _param) in &entry.sink_refs {
                let Some(sink) = node_by_name.get(sink_name.as_str()) else {
                    continue;
                };
                edges.push(GraphEdge {
                    cable: cable.clone(),
                    source: source.clone(),
                    sink: sink.clone(),
                });
            }
        }
    }
    // Sort deterministically: `patch.cable_index` is a HashMap, whose iteration
    // order is randomized per process. Edge order feeds the layout solver's
    // f32 spring-force accumulation (non-commutative under rounding) and the
    // renderer's shared-cell ownership, so a stable order is required for
    // reproducible layouts (design D9).
    edges.sort_by(|a, b| (&a.cable, &a.source, &a.sink).cmp(&(&b.cable, &b.source, &b.sink)));
    edges
}

/// Topology validation as a graph-build step (design D4). For every cable in
/// the patch's cable index, exactly one source is valid; zero sources (a
/// dangling reference: some section sinks a cable nobody produces) is a
/// `Warning`; two or more sources driving one cable is an invalid `n → 1`
/// topology and an `Error`.
///
/// A produced-but-unused cable (one source, no sinks) is fine: `n` is any
/// number of sinks. Findings travel with the graph for the renderer to
/// highlight; they never block building or viewing.
fn validate_topology(patch: &Patch) -> Vec<TopologyIssue> {
    let mut issues = Vec::new();
    for (cable, entry) in &patch.cable_index {
        match entry.sources.len() {
            0 => issues.push(TopologyIssue {
                cable: cable.clone(),
                severity: TopologySeverity::Warning,
                message: format!(
                    "cable {cable} is referenced as a sink by {} section(s) but never produced by an `output =`",
                    entry.sink_refs.len()
                ),
            }),
            1 => {}
            n => issues.push(TopologyIssue {
                cable: cable.clone(),
                severity: TopologySeverity::Error,
                message: format!(
                    "cable {cable} has {n} sources ({}) but exactly one is required",
                    entry.sources.join(", ")
                ),
            }),
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(content: &str, clusters: &[Cluster]) -> Graph {
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        Graph::build_from_patch(&patch, clusters)
    }

    #[test]
    fn node_set_matches_circuits_with_repeated_instances() {
        let graph = build(
            "[p2b8]\n\
             [clocktool]\n    output = _CLK\n\
             [copy]\n    input = _CLK\n\
             [copy]\n    input = _CLK\n\
             [osc]\n    input = _CLK\n",
            &[],
        );

        // Five sections → five nodes, including both [copy] instances.
        assert_eq!(graph.nodes.len(), 5);

        let copies: Vec<_> = graph.nodes.iter().filter(|n| n.circuit == "copy").collect();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0].instance_index, 0);
        assert_eq!(copies[1].instance_index, 1);
        assert_ne!(copies[0].id, copies[1].id);

        // Section indices are the distinct 0..n positions.
        let mut section_indices: Vec<usize> = graph.nodes.iter().map(|n| n.section_index).collect();
        section_indices.sort_unstable();
        assert_eq!(section_indices, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn edges_match_cable_fan_out() {
        let graph = build(
            "[p2b8]\n\
             [src]\n    output = _CLK\n\
             [sink1]\n    input = _CLK\n\
             [sink2]\n    input = _CLK\n\
             [sink3]\n    input = _CLK\n",
            &[],
        );

        // One source fanning out to three sinks → three directed edges.
        assert_eq!(graph.edges.len(), 3);
        for edge in &graph.edges {
            assert_eq!(edge.cable, "_CLK");
            assert_eq!(edge.source, (String::from("src"), 0));
        }
        let sinks: Vec<&NodeId> = graph.edges.iter().map(|e| &e.sink).collect();
        assert_eq!(
            sinks,
            vec![
                &(String::from("sink1"), 0),
                &(String::from("sink2"), 0),
                &(String::from("sink3"), 0),
            ]
        );
    }

    #[test]
    fn clusters_match_banner_ranges() {
        let clusters = vec![
            Cluster {
                title: String::from("Pulsar clock"),
                section_range: 1..2,
            },
            Cluster {
                title: String::from("Steady clock"),
                section_range: 2..3,
            },
        ];
        let graph = build(
            "[p2b8]\n[clocktool]\n    output = _CLK\n[osc]\n    input = _CLK\n",
            &clusters,
        );

        // Clusters are stored verbatim as passed.
        assert_eq!(graph.clusters.len(), 2);
        assert_eq!(graph.clusters[0].title, "Pulsar clock");
        assert_eq!(graph.clusters[0].section_range, 1..2);
        assert_eq!(graph.clusters[1].title, "Steady clock");
        assert_eq!(graph.clusters[1].section_range, 2..3);

        // A node's section index falls inside its cluster's range, so cluster
        // membership is derivable from the model.
        let clocktool = graph
            .nodes
            .iter()
            .find(|n| n.circuit == "clocktool")
            .unwrap();
        assert!(graph.clusters[0]
            .section_range
            .contains(&clocktool.section_index));
    }

    #[test]
    fn cable_without_source_produces_no_edge() {
        // `_ORPHAN` is only referenced as a sink, never produced: the build
        // produces no edge, and validation flags the dangling ref as a warning.
        let graph = build("[p2b8]\n[sink]\n    input = _ORPHAN\n", &[]);
        assert!(graph.edges.is_empty());
        assert_eq!(graph.validation.len(), 1);
        let issue = &graph.validation[0];
        assert_eq!(issue.cable, "_ORPHAN");
        assert_eq!(issue.severity, TopologySeverity::Warning);
    }

    #[test]
    fn valid_fanout_produces_no_validation_issues() {
        // One source fanning out to three sinks is the canonical valid case.
        let graph = build(
            "[p2b8]\n\
             [src]\n    output = _CLK\n\
             [sink1]\n    input = _CLK\n\
             [sink2]\n    input = _CLK\n\
             [sink3]\n    input = _CLK\n",
            &[],
        );
        assert!(graph.validation.is_empty());
    }

    #[test]
    fn dangling_cable_flags_warning() {
        // `_ORPHAN` is sunk by two sections but never produced.
        let graph = build(
            "[p2b8]\n\
             [a]\n    input = _ORPHAN\n\
             [b]\n    input = _ORPHAN\n",
            &[],
        );
        assert_eq!(graph.validation.len(), 1);
        let issue = &graph.validation[0];
        assert_eq!(issue.cable, "_ORPHAN");
        assert_eq!(issue.severity, TopologySeverity::Warning);
        // Two sink sections are reported in the message.
        assert!(issue.message.contains("2 section(s)"));
    }

    #[test]
    fn multiple_sources_flags_n_to_one_error() {
        // Two circuits both `output = _BUS`: invalid n → 1 topology.
        let graph = build(
            "[p2b8]\n\
             [prod1]\n    output = _BUS\n\
             [prod2]\n    output = _BUS\n\
             [sink]\n    input = _BUS\n",
            &[],
        );
        assert_eq!(graph.validation.len(), 1);
        let issue = &graph.validation[0];
        assert_eq!(issue.cable, "_BUS");
        assert_eq!(issue.severity, TopologySeverity::Error);
        // Both producers are named in the message.
        assert!(issue.message.contains("prod1") && issue.message.contains("prod2"));
    }

    #[test]
    fn mixed_cases_flag_appropriately() {
        // One valid fan-out cable, one dangling cable, one n → 1 cable.
        let graph = build(
            "[p2b8]\n\
             [src]\n    output = _A\n\
             [sink_a]\n    input = _A\n\
             [prod1]\n    output = _BUS\n\
             [prod2]\n    output = _BUS\n\
             [sink_bus]\n    input = _BUS\n\
             [dang]\n    input = _ORPHAN\n",
            &[],
        );
        assert_eq!(graph.validation.len(), 2);
        // Iteration over the cable index is a HashMap: match by cable, not order.
        let by_cable: HashMap<&str, TopologySeverity> = graph
            .validation
            .iter()
            .map(|i| (i.cable.as_str(), i.severity))
            .collect();
        assert_eq!(by_cable.get("_A"), None, "valid cable must not be flagged");
        assert_eq!(
            by_cable.get("_BUS"),
            Some(&TopologySeverity::Error),
            "n → 1 cable must be an error"
        );
        assert_eq!(
            by_cable.get("_ORPHAN"),
            Some(&TopologySeverity::Warning),
            "dangling cable must be a warning"
        );
    }
}
/// Fixture-driven suite through the public `Graph::build_from_patch` entry
/// point (task 2.3). Model shape, edge directions, and cluster membership are
/// exercised against real `fixtures/` patches; synthetic inputs are used only
/// where a fixture lacks the needed shape (named banners, invalid topologies).
#[cfg(test)]
mod fixture_tests {
    use super::*;
    use std::path::Path;

    /// Load a fixture and build its graph, deriving clusters from the patch's
    /// own `banner_groups` exactly as a caller would (task 1.2).
    fn fixture_graph(name: &str) -> Graph {
        let patch = Patch::from_ini_file(Path::new(&format!("fixtures/{name}"))).unwrap();
        let clusters: Vec<Cluster> = patch
            .banner_groups
            .iter()
            .map(|g| Cluster {
                title: g.banner.clone().unwrap_or_default(),
                section_range: g.section_range.clone(),
            })
            .collect();
        Graph::build_from_patch(&patch, &clusters)
    }

    /// Cluster → `(title, section_range)` snapshot for a graph.
    fn cluster_spans(graph: &Graph) -> Vec<(String, Range<usize>)> {
        graph
            .clusters
            .iter()
            .map(|c| (c.title.clone(), c.section_range.clone()))
            .collect()
    }

    #[test]
    fn arpeggio_model_shape_matches_circuit_instances() {
        // 14 sections → 14 nodes; repeated [button] (8) and [copy] (2) names
        // are distinct instances with zero-based indices.
        let graph = fixture_graph("arpeggio1.ini");
        assert_eq!(graph.nodes.len(), 14);

        let buttons: Vec<&GraphNode> = graph
            .nodes
            .iter()
            .filter(|n| n.circuit == "button")
            .collect();
        assert_eq!(buttons.len(), 8);
        for (i, b) in buttons.iter().enumerate() {
            assert_eq!(b.instance_index, i);
            assert_eq!(b.id, (String::from("button"), i));
        }

        let copies: Vec<&GraphNode> = graph.nodes.iter().filter(|n| n.circuit == "copy").collect();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0].instance_index, 0);
        assert_eq!(copies[1].instance_index, 1);
        assert_ne!(copies[0].id, copies[1].id);

        // Section indices are the distinct 0..14 positions.
        let mut section_indices: Vec<usize> = graph.nodes.iter().map(|n| n.section_index).collect();
        section_indices.sort_unstable();
        assert_eq!(section_indices, (0..14).collect::<Vec<_>>());

        // Canonical circuit set (single occurrence of each distinct name).
        let mut circuits: Vec<&str> = graph.nodes.iter().map(|n| n.circuit.as_str()).collect();
        circuits.sort_unstable();
        circuits.dedup();
        assert_eq!(
            circuits,
            vec!["arpeggio", "button", "contour", "copy", "lfo", "p2b8"]
        );
    }

    #[test]
    fn arpeggio_edge_directions_run_from_button_sources_to_arpeggio() {
        // Each virtual cable is produced by a [button] and consumed by the
        // [arpeggio] section; _SCALE fans out to four select params.
        let graph = fixture_graph("arpeggio1.ini");
        assert_eq!(graph.edges.len(), 11);

        for e in &graph.edges {
            assert_eq!(
                e.source,
                (String::from("button"), 0),
                "cable {} must be produced by the first button instance",
                e.cable
            );
            assert_eq!(
                e.sink,
                (String::from("arpeggio"), 0),
                "cable {} must be consumed by the arpeggio section",
                e.cable
            );
        }

        // _SCALE reaches four arpeggio select params (fan-out within the patch).
        let scale_edges: Vec<&GraphEdge> =
            graph.edges.iter().filter(|e| e.cable == "_SCALE").collect();
        assert_eq!(scale_edges.len(), 4);
        for e in &scale_edges {
            assert_eq!(e.sink, (String::from("arpeggio"), 0));
        }
    }

    #[test]
    fn alg27_model_shape_matches_164_sections_with_repeated_instances() {
        // 164 sections across 22 distinct circuit names; repeated names get
        // unique ids rather than colliding.
        let graph = fixture_graph("alg27_2.ini");
        assert_eq!(graph.nodes.len(), 164);

        let mut section_indices: Vec<usize> = graph.nodes.iter().map(|n| n.section_index).collect();
        section_indices.sort_unstable();
        assert_eq!(section_indices, (0..164).collect::<Vec<_>>());

        let clocktools: Vec<&GraphNode> = graph
            .nodes
            .iter()
            .filter(|n| n.circuit == "clocktool")
            .collect();
        assert_eq!(clocktools.len(), 11);
        let ids: std::collections::HashSet<&NodeId> = clocktools.iter().map(|n| &n.id).collect();
        assert_eq!(
            ids.len(),
            11,
            "each clocktool instance must have a distinct id"
        );
        for (i, ct) in clocktools.iter().enumerate() {
            assert_eq!(ct.instance_index, i);
        }
    }

    #[test]
    fn alg27_pulsarclock_fans_out_twelve_edges_from_clocktool() {
        // [clocktool] output = _PULSARCLOCK reaches 12 real sinks: the two
        // [copy] inputs and the ten [clocktool] clock params (which resolve by
        // name back to the first clocktool instance → self loops).
        let graph = fixture_graph("alg27_2.ini");
        let clk: Vec<&GraphEdge> = graph
            .edges
            .iter()
            .filter(|e| e.cable == "_PULSARCLOCK")
            .collect();
        assert_eq!(clk.len(), 12);
        for e in &clk {
            assert_eq!(e.source, (String::from("clocktool"), 0));
        }
        let sinks: std::collections::HashSet<&NodeId> = clk.iter().map(|e| &e.sink).collect();
        assert_eq!(sinks.len(), 2, "sinks resolve to copy and clocktool only");
        assert!(sinks.contains(&(String::from("clocktool"), 0)));
        assert!(sinks.contains(&(String::from("copy"), 0)));
    }

    #[test]
    fn alg27_matrixsel_fans_out_seven_edges_from_pot_to_matrixmixer() {
        // `output = _MATRIXSEL` lives in a [pot] section; it is consumed by
        // seven [matrixmixer] select params → one source, seven directed edges.
        let graph = fixture_graph("alg27_2.ini");
        let mx: Vec<&GraphEdge> = graph
            .edges
            .iter()
            .filter(|e| e.cable == "_MATRIXSEL")
            .collect();
        assert_eq!(mx.len(), 7);
        for e in &mx {
            assert_eq!(e.source, (String::from("pot"), 0));
            assert_eq!(e.sink, (String::from("matrixmixer"), 0));
        }
    }

    #[test]
    fn cluster_membership_covers_every_node_via_fixture_banner_groups() {
        // Real fixtures carry no named banners, so each yields one implicit
        // unnamed group spanning all sections; every node's section_index must
        // land inside some cluster's range (membership derivable from the model).
        for name in ["arpeggio1.ini", "alg27_2.ini", "source_navigation.ini"] {
            let graph = fixture_graph(name);
            assert_eq!(graph.clusters.len(), 1, "{name} has no named banners");
            for node in &graph.nodes {
                assert!(
                    graph
                        .clusters
                        .iter()
                        .any(|c| c.section_range.contains(&node.section_index)),
                    "{name}: node {} (section {}) not covered by any cluster",
                    node.circuit,
                    node.section_index
                );
            }
        }
    }

    #[test]
    fn cluster_membership_matches_named_banner_ranges_synthetic() {
        // Fixtures lack named banners, so exercise multi-banner membership on a
        // synthetic patch (allowed by the task where the fixture lacks the shape).
        let content = "\
# ---- Pulsar ----
[clocktool]
    output = _CLK
[copy]
    input = _CLK
# ---- Steady ----
[osc]
    input = _CLK
[p2b8]
";
        let patch = Patch::from_ini_str(content, String::from("clusters")).unwrap();
        let clusters: Vec<Cluster> = patch
            .banner_groups
            .iter()
            .map(|g| Cluster {
                title: g.banner.clone().unwrap_or_default(),
                section_range: g.section_range.clone(),
            })
            .collect();
        let graph = Graph::build_from_patch(&patch, &clusters);

        assert_eq!(
            cluster_spans(&graph),
            vec![
                (String::from("Pulsar"), 0..2),
                (String::from("Steady"), 2..4),
            ]
        );

        let clocktool = graph
            .nodes
            .iter()
            .find(|n| n.circuit == "clocktool")
            .unwrap();
        let osc = graph.nodes.iter().find(|n| n.circuit == "osc").unwrap();
        assert!(graph.clusters[0]
            .section_range
            .contains(&clocktool.section_index));
        assert!(graph.clusters[1].section_range.contains(&osc.section_index));
        assert!(!graph.clusters[1]
            .section_range
            .contains(&clocktool.section_index));
    }

    #[test]
    fn arpeggio_fixture_is_topologically_valid() {
        // All eight real cables have exactly one source → no validation issues.
        let graph = fixture_graph("arpeggio1.ini");
        assert!(
            graph.validation.is_empty(),
            "arpeggio1.ini must be valid, got {:?}",
            graph.validation
        );
    }

    #[test]
    fn alg27_fixture_flags_dangling_cable_as_warning() {
        // `_CHANSEL` is consumed in real params but produced by no `output =`:
        // a genuine dangling reference (externally sourced in the real patch).
        // Cables with exactly one source (_PULSARCLOCK, _MATRIXSEL, _MATRIXEDIT)
        // are not flagged.
        let graph = fixture_graph("alg27_2.ini");
        let by_cable: HashMap<&str, TopologySeverity> = graph
            .validation
            .iter()
            .map(|i| (i.cable.as_str(), i.severity))
            .collect();
        assert_eq!(
            by_cable.get("_CHANSEL"),
            Some(&TopologySeverity::Warning),
            "dangling _CHANSEL must be a warning"
        );
        assert_eq!(by_cable.get("_PULSARCLOCK"), None);
        assert_eq!(by_cable.get("_MATRIXSEL"), None);
        assert_eq!(by_cable.get("_MATRIXEDIT"), None);
    }
}
