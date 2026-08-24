//! Signal-flow graph model: circuits as nodes, virtual cables as directed edges.
//!
//! Pure module (design D5) — no terminal dependency, so it is testable without
//! rendering. `build_from_patch` turns a parsed `Patch` (cable index + sections)
//! plus caller-supplied banner clusters into a graph the renderer can draw and
//! the layout solver can position.

use std::collections::HashMap;
use std::ops::Range;

use crate::patch::Patch;

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
