//! Signal-flow graph model: circuits as nodes, virtual cables as directed edges.
//!
//! Pure module (design D5) — no terminal dependency, so it is testable without
//! rendering. `build_from_patch` turns a parsed `Patch` (cable index + sections)
//! plus caller-supplied banner clusters into a graph the renderer can draw and
//! the layout solver can position.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;

use crate::geometry::{BindingFeatures, RackGeometry, WiringOutlierScorer};
use crate::latency::{forward_latency, CostModel, LatencyData};
use crate::patch::{scan_internal_tokens, scan_register_refs, InfluenceSubtree, Patch};
use crate::schema::load_schema;

// Euclidean-distance wiring-outlier detection is delegated to the learned
// decision table (`geometry::WiringOutlierScorer`, embedded artifact from
// `tools/fit_outlier_model.py`) with a preserved threshold fallback — see
// `validate_wiring_outliers`. Invariant guards (adjacent / co-located / via-
// cable) stay explicit at the call site and never reach the scorer.

/// Node identity: `Circuit(name, idx)`, `Controller(type, ordinal)`, or
/// `Jack(token)`. Defined in `patch` (the influence walk needs it and `patch`
/// must not depend on `graph`); re-exported here for graph consumers.
pub use crate::patch::NodeId;

/// The kind of a graph node: a circuit section, a declared controller, or a
/// master input/output jack. All nodes render as circuits until task 4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Circuit,
    Controller,
    InputJack,
    OutputJack,
}

/// A circuit (section) rendered as a graph node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: NodeId,
    /// Node kind. Always `Circuit` today; controller/jack nodes arrive in
    /// task 3.1.
    pub kind: NodeKind,
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

impl GraphEdge {
    /// Index of this edge's source node in `nodes`, if present (change D task
    /// 1.1 subset mapping).
    pub fn source_index(&self, nodes: &[GraphNode]) -> Option<usize> {
        nodes.iter().position(|n| n.id == self.source)
    }

    /// Index of this edge's sink node in `nodes`, if present (change D task
    /// 1.1 subset mapping).
    pub fn sink_index(&self, nodes: &[GraphNode]) -> Option<usize> {
        nodes.iter().position(|n| n.id == self.sink)
    }
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
    /// Forward-loop latency (design D1/D2): one [`EdgeLatency`] per edge,
    /// parallel to `edges` by index, plus the aggregate summary. Computed at
    /// build time by [`Graph::build_from_patch`] like `validate_topology`;
    /// `None` for hand-built or empty graphs (no nodes or no edges).
    pub latency: Option<LatencyData>,
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
    /// Section indices of circuits classified NotSelected by the select-state
    /// filter (change C 4.1). The renderer dims these nodes; their controller
    /// register edges are already absent from `edges`. Empty when no select
    /// state is assumed.
    pub not_selected: HashSet<usize>,
}

/// Options for `Graph::build_from_patch` controlling select-state filtering.
#[derive(Debug, Clone, Default)]
pub struct GraphOptions {
    pub state: HashMap<String, f64>,
    pub hide_unselected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectClass {
    Selected,
    NotSelected,
    Unknown,
}

fn classify_sections(patch: &Patch, options: &GraphOptions) -> Vec<SelectClass> {
    patch
        .sections
        .iter()
        .map(|section| {
            let select_value = section
                .entries
                .iter()
                .find(|(k, _)| k.to_lowercase() == "select")
                .map(|(_, v)| v.as_str());
            let Some(raw) = select_value else {
                return SelectClass::Selected;
            };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return SelectClass::Unknown;
            }
            let root: Option<String> = {
                let regs = scan_register_refs(trimmed);
                if regs.len() == 1 && regs[0].0 == trimmed {
                    Some(regs[0].0.clone())
                } else if trimmed.starts_with('_') {
                    let cables = scan_internal_tokens(trimmed);
                    if cables.len() == 1 && cables[0] == trimmed {
                        Some(cables[0].clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            let Some(root) = root else {
                return SelectClass::Unknown;
            };
            // If the section has a `selectat` value, the circuit is selected
            // when that literal equals the assumed value for the root signal.
            // This matches DROID's `select` / `selectat` wiring where a shared
            // switch chooses one of several circuits by value.
            if let Some(selectat_raw) = section
                .entries
                .iter()
                .find(|(k, _)| k.to_lowercase() == "selectat")
                .map(|(_, v)| v.trim())
            {
                if let Some(v) =
                    crate::expression::evaluate_droid_expr(selectat_raw, &HashMap::new())
                        .or_else(|| selectat_raw.parse::<f64>().ok())
                {
                    match options.state.get(&root) {
                        Some(assumed) if (v - assumed).abs() < 1e-9 => {
                            return SelectClass::Selected
                        }
                        Some(_) => return SelectClass::NotSelected,
                        None => return SelectClass::Unknown,
                    }
                }
            }
            let Some(eval) = crate::expression::evaluate_droid_expr(trimmed, &options.state) else {
                return SelectClass::Unknown;
            };
            match options.state.get(&root) {
                Some(assumed) if (eval - assumed).abs() < 1e-9 => SelectClass::Selected,
                Some(_) => SelectClass::NotSelected,
                None => SelectClass::Unknown,
            }
        })
        .collect()
}

impl Graph {
    /// Build a signal-flow graph from a parsed `Patch`.
    ///
    /// - Every section becomes a node; repeated names are distinct instances.
    /// - Each cable in `patch.cable_index` fans its source out to every sink
    ///   reference, producing one directed edge per (cable, sink).
    /// - `clusters` are stored verbatim; callers pass banner groups derived
    ///   from `Patch.banner_groups` (task 1.2).
    /// - `cost` is the shared per-circuit cost provider (design D2) feeding
    ///   the latency ramp; the caller (typically `App`) owns one built from
    ///   `[latency]` config, keeping this module pure.
    ///
    /// Cable attribution is by section *name* (the cable index records names,
    /// not instance indices), so a name shared by several instances resolves
    /// to the first instance. Instance-accurate attribution is left to the
    /// topology-validation pass (task 2.2), which operates on the cable index
    /// entries by name, keeping that convention consistent.
    pub fn build_from_patch(
        patch: &Patch,
        clusters: &[Cluster],
        cost: &CostModel,
        options: &GraphOptions,
    ) -> Graph {
        let classes = classify_sections(patch, options);
        let not_selected: HashSet<usize> = classes
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                if *c == SelectClass::NotSelected {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        let mut nodes = build_nodes(patch);
        let node_to_section: HashMap<NodeId, usize> = nodes
            .iter()
            .map(|n| (n.id.clone(), n.section_index))
            .collect();
        let node_by_name = name_to_first_node(&nodes);
        let cable_edges = build_edges(patch, &node_by_name);

        let validation = validate_topology(patch);
        // Latency measures signal-path cables only — compute from circuit
        // nodes before registering controller/jack register nodes.
        let latency = compute_latency(&nodes, &cable_edges, cost);

        // Register edges: controller/jack nodes + register connections.
        let (reg_nodes, reg_edges) = build_register_nodes_and_edges(patch);
        let filtered_reg_edges: Vec<GraphEdge> = reg_edges
            .into_iter()
            .filter(|e| {
                let circuit_id_opt = if matches!(e.source, NodeId::Circuit(_, _)) {
                    Some(&e.source)
                } else if matches!(e.sink, NodeId::Circuit(_, _)) {
                    Some(&e.sink)
                } else {
                    None
                };
                let Some(circuit_id) = circuit_id_opt else {
                    return true;
                };
                let Some(&sec_idx) = node_to_section.get(circuit_id) else {
                    return true;
                };
                if !not_selected.contains(&sec_idx) {
                    return true;
                }
                let other = if circuit_id == &e.source {
                    &e.sink
                } else {
                    &e.source
                };
                !matches!(other, NodeId::Controller(_, _))
            })
            .collect();
        nodes.extend(reg_nodes);
        if options.hide_unselected {
            nodes.retain(|n| {
                if n.kind != NodeKind::Circuit {
                    return true;
                }
                !not_selected.contains(&n.section_index)
            });
        }

        // Merge cable + register edges, sorted deterministically.
        let mut edges = cable_edges;
        edges.extend(filtered_reg_edges);
        edges.sort_by(|a, b| (&a.cable, &a.source, &a.sink).cmp(&(&b.cable, &b.source, &b.sink)));

        Graph {
            nodes,
            edges,
            clusters: clusters.to_vec(),
            validation,
            latency,
            highlighted_nodes: HashSet::new(),
            highlighted_edges: HashSet::new(),
            not_selected,
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

    /// Index of the banner-group cluster whose `section_range` contains
    /// `section_index`, if any. This is the cluster-membership mapping the
    /// layout solver consumes for per-cluster cohesion (design D3): a node
    /// belongs to the cluster owning its `Patch.sections` position. Returns
    /// `None` for a section in no banner group (or an unknown index), so the
    /// solver can skip the cohesion force defensively instead of panicking.
    pub fn cluster_index_of(&self, section_index: usize) -> Option<usize> {
        self.clusters
            .iter()
            .position(|c| c.section_range.contains(&section_index))
    }

    /// Every node that feeds `root` transitively, following incoming edges
    /// (reversed) breadth-first; returns indices into `self.nodes` in BFS
    /// order, root first (change D task 1.1).
    ///
    /// Each node is visited once, so cycles terminate. The walk stops at
    /// controller and input-jack nodes: their incoming edges (LED writes,
    /// button feedback) are not dependencies. A controller or input-jack root
    /// yields only itself.
    pub fn upstream_dependencies(&self, root: &NodeId) -> Vec<usize> {
        let Some(root_idx) = self.nodes.iter().position(|n| &n.id == root) else {
            return Vec::new();
        };
        if matches!(
            self.nodes[root_idx].kind,
            NodeKind::Controller | NodeKind::InputJack
        ) {
            // Controller and input-jack nodes are sources: nothing feeds them.
            return vec![root_idx];
        }

        // Reverse adjacency: sink -> producing sources (built per call; the
        // graph is at DROID scale, and this runs once per root toggle).
        let mut incoming: HashMap<&NodeId, Vec<usize>> = HashMap::new();
        for (ei, edge) in self.edges.iter().enumerate() {
            incoming.entry(&edge.sink).or_default().push(ei);
        }

        let mut visited: HashSet<usize> = HashSet::new();
        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut out: Vec<usize> = Vec::new();
        visited.insert(root_idx);
        queue.push_back(root_idx);
        while let Some(idx) = queue.pop_front() {
            out.push(idx);
            if matches!(
                self.nodes[idx].kind,
                NodeKind::Controller | NodeKind::InputJack
            ) {
                // Controller/input-jack leaf: never follow its incoming edges.
                continue;
            }
            let Some(producers) = incoming.get(&self.nodes[idx].id) else {
                continue;
            };
            let mut sources: Vec<usize> = producers
                .iter()
                .map(|ei| {
                    self.edges[*ei]
                        .source_index(&self.nodes)
                        .expect("edge source always resolves")
                })
                .collect();
            sources.sort_unstable();
            sources.dedup();
            for s in sources {
                if visited.insert(s) {
                    queue.push_back(s);
                }
            }
        }
        out
    }

    /// Edge indices whose endpoints are both in `members` (a node-index set) -
    /// the internal edges of a subgraph (change D task 1.1).
    pub fn internal_edges(&self, members: &HashSet<usize>) -> Vec<usize> {
        self.edges
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let src = e.source_index(&self.nodes);
                let sink = e.sink_index(&self.nodes);
                src.is_some_and(|s| members.contains(&s))
                    && sink.is_some_and(|s| members.contains(&s))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

/// Compute forward-loop latency as a graph-build step (design D2), mirroring
/// `validate_topology`. `None` when there is nothing to measure (no nodes or
/// no edges); otherwise `Some` with one [`LatencyData`] per edge, in
/// `Graph.edges` order.
///
/// AVG is RAM-derived (design D1/spec): `AVG(circuit) ∝ ramsize(circuit)`,
/// scaled by the largest schema `available_memory` value so a patch filling
/// the loop budget maps to the ~190µs loop. Unknown circuits (not in the
/// schema) fall back to unit cost 1.0 — only synthetic test graphs reach it,
/// since a validated patch cannot contain unknown circuits. Lookups are
/// `HashMap::get` by circuit name (never iteration order) and edges are
/// iterated by index, so the result is deterministic across runs.
fn compute_latency(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    cost: &CostModel,
) -> Option<LatencyData> {
    if nodes.is_empty() || edges.is_empty() {
        return None;
    }
    // File order == processing order; `section_index` is the processing
    // position consumed by `forward_latency`.
    let node_positions: Vec<(NodeId, usize)> = nodes
        .iter()
        .map(|n| (n.id.clone(), n.section_index))
        .collect();
    let schema = load_schema();
    // The provider merges config `[latency]` overrides over the
    // ramsize-proportional heuristic; identical input stays deterministic.
    let circuit_avg = |id: &NodeId| cost.circuit_avg(id, schema);
    let (lat_edges, summary) = forward_latency(edges, &node_positions, circuit_avg);
    Some(LatencyData {
        edges: lat_edges,
        summary,
    })
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
                id: NodeId::circuit(&section.name, instance_index),
                kind: NodeKind::Circuit,
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
    edges.sort_by(|a, b| (&a.cable, &a.source, &a.sink).cmp(&(&b.cable, &b.source, &b.sink)));
    edges
}

/// Controller-register prefix letters that map to on-controller register
/// families (buttons, pots/faders, encoders, switches, LEDs, R). Letters
/// outside this set (I, O, G, M, N) are master-jack families.
const CONTROLLER_REGISTER_PREFIXES: &str = "PBESLR";

/// Build controller/jack nodes and register edges from the patch's controller
/// list and section entries.
///
/// Register edges connect circuits to the physical controls (buttons, knobs,
/// LEDs, jacks) they reference. Direction comes from the schema catalog: an
/// output parameter writes its register refs (circuit → target), an input
/// parameter reads them (target → circuit). Unknown-circuit fallback follows
/// the `output`/`led` key convention.
///
/// A register belongs to a declared controller when its letter is in PBESLR
/// and a controller with that unit number exists; otherwise it is a jack on
/// the master (input when the letter is I, N, or a controller-register
/// letter without a matching unit, output otherwise).
fn build_register_nodes_and_edges(patch: &Patch) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let schema = load_schema();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen_jacks: HashSet<String> = HashSet::new();
    let mut seen_edges: HashSet<(NodeId, NodeId, String)> = HashSet::new();

    // 1. Create controller nodes from the patch's controller list.
    for ctrl in &patch.controllers {
        nodes.push(GraphNode {
            id: NodeId::Controller(ctrl.name.clone(), ctrl.ordinal as usize),
            kind: NodeKind::Controller,
            circuit: ctrl.panel.clone(),
            instance_index: ctrl.ordinal as usize,
            section_index: 0,
        });
    }

    // 2. Build occurrence-counted node IDs for each section so register edges
    //    target the correct circuit instance.
    let mut instance_counts: HashMap<&str, usize> = HashMap::new();
    let section_node_ids: Vec<NodeId> = patch
        .sections
        .iter()
        .map(|section| {
            let count = instance_counts.entry(section.name.as_str()).or_insert(0);
            let idx = *count;
            *count += 1;
            NodeId::circuit(&section.name, idx)
        })
        .collect();

    // 3. Scan each section's entries for register references and build edges.
    for (section_idx, section) in patch.sections.iter().enumerate() {
        let circuit = &section.name;
        let circuit_node = &section_node_ids[section_idx];

        for (key, value) in &section.entries {
            let is_output = schema
                .get_param_kind(circuit, key)
                .map(|k| k == "output")
                .unwrap_or_else(|| {
                    // Fallback: output/led families write, everything else reads.
                    let lk = key.to_lowercase();
                    lk.contains("output") || lk.contains("led")
                });

            for (token, unit, _pin) in scan_register_refs(value) {
                let prefix = match token.chars().next() {
                    Some(c) => c,
                    None => continue,
                };

                // Classify target: controller node or jack node.
                let upper = prefix.to_ascii_uppercase();
                let target = if CONTROLLER_REGISTER_PREFIXES.contains(upper)
                    && patch.controllers.iter().any(|c| c.ordinal == unit)
                {
                    let ctrl = patch
                        .controllers
                        .iter()
                        .find(|c| c.ordinal == unit)
                        .unwrap();
                    NodeId::Controller(ctrl.name.clone(), ctrl.ordinal as usize)
                } else {
                    NodeId::Jack(token.clone())
                };

                // Create jack nodes on demand.
                if let NodeId::Jack(ref tok) = target {
                    if seen_jacks.insert(tok.clone()) {
                        let kind = if upper == 'I'
                            || upper == 'N'
                            || (CONTROLLER_REGISTER_PREFIXES.contains(upper)
                                && !patch.controllers.iter().any(|c| c.ordinal == unit))
                        {
                            NodeKind::InputJack
                        } else {
                            NodeKind::OutputJack
                        };
                        nodes.push(GraphNode {
                            id: NodeId::Jack(tok.clone()),
                            kind,
                            circuit: tok.clone(),
                            instance_index: 0,
                            section_index: 0,
                        });
                    }
                }

                // Build edge with direction from catalog.
                let (src, sink) = if is_output {
                    (circuit_node.clone(), target)
                } else {
                    (target, circuit_node.clone())
                };

                let cable = format!("_REG:{token}");
                if seen_edges.insert((src.clone(), sink.clone(), cable.clone())) {
                    edges.push(GraphEdge {
                        cable,
                        source: src,
                        sink,
                    });
                }
            }
        }
    }

    // Sort deterministically (same contract as build_edges).
    edges.sort_by(|a, b| (&a.cable, &a.source, &a.sink).cmp(&(&b.cable, &b.source, &b.sink)));
    (nodes, edges)
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
    // ---- wiring-outlier check (Track 1 hard invariant) ----
    // Best-effort: if geometry fails to load (e.g. missing file in tests),
    // skip the check without failing topology validation.
    if let Ok(geometry) = RackGeometry::load() {
        issues.extend(validate_wiring_outliers(patch, &geometry));
    }
    // ---- per-token influence second opinion (design D4) ----
    issues.extend(validate_influence_outliers(patch));
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

fn validate_wiring_outliers(patch: &Patch, geometry: &RackGeometry) -> Vec<TopologyIssue> {
    let scorer = WiringOutlierScorer::embedded();
    let mut outliers = Vec::new();
    let mut seen_pairs: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for section in &patch.sections {
        let mut tokens: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (_, value) in &section.entries {
            for tok in scan_hw_tokens_for_graph(value) {
                if seen.insert(tok.clone()) {
                    // Only consider tokens the geometry can resolve
                    if geometry.resolve(&tok).is_some() {
                        tokens.push(tok);
                    }
                }
            }
        }
        if tokens.len() < 2 {
            continue;
        }
        tokens.sort();
        // Unordered pairs within the same section = direct wiring
        for i in 0..tokens.len() {
            for j in (i + 1)..tokens.len() {
                let a = &tokens[i];
                let b = &tokens[j];
                // Canonical ordering to deduplicate across sections
                let key = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                if !seen_pairs.insert(key.clone()) {
                    continue;
                }
                let Some(feat) = BindingFeatures::from_tokens(a, b, geometry, patch) else {
                    continue;
                };
                // Invariant guards (design D5) — hard guarantees, never
                // learned: adjacent, co-located L->B (distance 0) and
                // via-cable bindings never reach the scorer.
                if feat.adjacent || feat.euclidean < 1e-6 || feat.cable_hops != 0 {
                    continue;
                }
                if scorer.is_outlier(&feat) {
                    outliers.push(TopologyIssue {
                        cable: format!("{}->{}", a, b),
                        severity: TopologySeverity::Warning,
                        message: format!(
                            "wiring outlier: {} -> {} distance {:.1} hops {} (decision table)",
                            a, b, feat.euclidean, feat.cable_hops
                        ),
                    });
                }
            }
        }
    }
    outliers
}

/// z-score band: a token whose influence-subtree size exceeds mean + 3σ for
/// its kind is flagged as a `Warning` (design D4 second opinion). Calibrated
/// against the corpus stats embedded from tools/build_features.py.
const INFLUENCE_ZSCORE_BAND: f32 = 3.0;

/// Per-token influence second opinion (design D4): every distinct hardware
/// token in the patch is z-scored against the embedded per-kind corpus stats
/// of influence-subtree size. Tokens beyond the calibrated band produce a
/// `TopologyIssue` Warning attached to the token's first root `_VAR` cable so
/// the renderer's error-highlight token colors it. Never gates patch loading
/// (warnings only). Deterministic: tokens sorted, first root var (sorted).
fn validate_influence_outliers(patch: &Patch) -> Vec<TopologyIssue> {
    let mut tokens: Vec<&str> = patch.hw_components.iter().map(|c| c.id.as_str()).collect();
    tokens.sort_unstable();
    tokens.dedup();
    let mut issues = Vec::new();
    for token in tokens {
        let Some(z) = patch.token_influence_z_score(token) else {
            continue;
        };
        if z <= INFLUENCE_ZSCORE_BAND {
            continue;
        }
        let vars = patch.hw_token_to_vars(token);
        let cable = vars.first().cloned().unwrap_or_else(|| token.to_string());
        let size = patch.influence_subtree_size_for(token);
        issues.push(TopologyIssue {
            cable,
            severity: TopologySeverity::Warning,
            message: format!(
                "influence outlier: token {token} reaches {size} circuit(s), z={z:.1} (band {INFLUENCE_ZSCORE_BAND:.0})"
            ),
        });
    }
    issues
}

const HW_TOKEN_LETTERS_GRAPH: [char; 10] = ['B', 'L', 'P', 'O', 'I', 'E', 'S', 'M', 'R', 'G'];

fn scan_hw_tokens_for_graph(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let boundary_ok = i == 0 || !(chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_');
        let starts_token = HW_TOKEN_LETTERS_GRAPH.contains(&c)
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_digit()
            && boundary_ok;
        if starts_token {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len()
                && chars[i] == '.'
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let clean_end = i >= chars.len()
                || !(chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.');
            if clean_end {
                tokens.push(chars[start..i].iter().collect());
            }
            continue;
        }
        i += 1;
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(content: &str, clusters: &[Cluster]) -> Graph {
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        Graph::build_from_patch(
            &patch,
            clusters,
            &CostModel::default(),
            &GraphOptions::default(),
        )
    }

    /// Minimal schema fixture for catalog-driven edge direction: circuit `pulser`
    /// declares a nonstandard output param `pulse` and an input param `input`.
    /// Leaked to `&'static` for `set_test_schema`; sibling of the patch.rs one so
    /// the graph test does not reach into that module's private fixture.
    fn catalog_test_schema() -> &'static crate::schema::Schema {
        let schema: crate::schema::Schema = serde_json::from_str(
            r#"{
            "firmware_version": "test",
            "jacktable_initial_size": 0,
            "available_memory": {},
            "circuits": {
                "pulser": {
                    "category": "test",
                    "title": "Pulser",
                    "description": "test circuit",
                    "ramsize": 100,
                    "inputs": [
                        {
                            "name": "input",
                            "short": "in",
                            "type": "in",
                            "description": "input",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "outputs": [
                        {
                            "name": "pulse",
                            "short": "pulse",
                            "type": "out",
                            "description": "output",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "presets": 0,
                    "manual": 0
                },
                "buttonwriter": {
                    "category": "test",
                    "title": "Button Writer",
                    "description": "writes to button registers",
                    "ramsize": 10,
                    "inputs": [],
                    "outputs": [
                        {
                            "name": "button",
                            "short": "btn",
                            "type": "out",
                            "description": "button output",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "presets": 0,
                    "manual": 0
                },
                "potreader": {
                    "category": "test",
                    "title": "Pot Reader",
                    "description": "reads from pot registers",
                    "ramsize": 10,
                    "inputs": [
                        {
                            "name": "pot",
                            "short": "pot",
                            "type": "in",
                            "description": "pot input",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "outputs": [],
                    "presets": 0,
                    "manual": 0
                }
            },
            "controllers": {},
            "manual_references": {}
        }"#,
        )
        .unwrap();
        Box::leak(Box::new(schema))
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

        // Five sections → five circuit nodes, including both [copy] instances.
        // (The [p2b8] section also creates a Controller node.)
        let circuit_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Circuit)
            .collect();
        assert_eq!(circuit_nodes.len(), 5);
        let copies: Vec<_> = circuit_nodes
            .iter()
            .filter(|n| n.circuit == "copy")
            .collect();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0].instance_index, 0);
        assert_eq!(copies[1].instance_index, 1);
        assert_ne!(copies[0].id, copies[1].id);

        // Section indices are the distinct 0..n positions (circuit nodes only).
        let section_indices: Vec<usize> = circuit_nodes.iter().map(|n| n.section_index).collect();
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
            assert_eq!(edge.source, NodeId::circuit("src", 0));
        }
        let sinks: Vec<&NodeId> = graph.edges.iter().map(|e| &e.sink).collect();
        assert_eq!(
            sinks,
            vec![
                &NodeId::circuit("sink1", 0),
                &NodeId::circuit("sink2", 0),
                &NodeId::circuit("sink3", 0),
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
    fn catalog_driven_edge_direction() {
        // `pulser`'s `pulse` is a catalog output and `input` a catalog input, so
        // the graph must produce `_OUT` from `pulser` and consume `_IN` at
        // `pulser`, even though neither key is the conventional `output`. A
        // second (catalog-unknown) circuit completes both cables so real directed
        // edges exist. Pin the fixture schema for the parse, then restore so a
        // failing assert cannot leak the override to later tests on this thread.
        let schema = catalog_test_schema();
        crate::schema::set_test_schema(Some(schema));
        let graph = build(
            "[p2b8]\n\
             [pulser]\n    pulse = _OUT\n    input = _IN\n\
             [sink]\n    input = _OUT\n    output = _IN\n",
            &[],
        );
        crate::schema::reset_test_schema();

        // The catalog output `pulse` is the source of `_OUT`, fanned to `sink`.
        let out_edge = graph
            .edges
            .iter()
            .find(|e| e.cable == "_OUT")
            .expect("_OUT edge exists");
        assert_eq!(out_edge.source, NodeId::circuit("pulser", 0));
        assert_eq!(out_edge.sink, NodeId::circuit("sink", 0));

        // The catalog input `input` consumes `_IN` at `pulser`, produced by
        // `sink`'s conventional `output`.
        let in_edge = graph
            .edges
            .iter()
            .find(|e| e.cable == "_IN")
            .expect("_IN edge exists");
        assert_eq!(in_edge.source, NodeId::circuit("sink", 0));
        assert_eq!(in_edge.sink, NodeId::circuit("pulser", 0));

        // Exactly the two directed edges, so direction comes from the catalog.
        assert_eq!(graph.edges.len(), 2);
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

    #[test]
    fn wiring_outlier_far_direct_flagged() {
        // E4.4 (left encoder) directly to M4.2 (fader) without cable -> flagged
        let graph = build("[p2b8]\n[copy]\n    src = E4.4\n    dst = M4.2\n", &[]);
        let outlier = graph
            .validation
            .iter()
            .find(|iss| iss.message.contains("wiring outlier"));
        assert!(
            outlier.is_some(),
            "far direct E4.4->M4.2 must be flagged, got {:?}",
            graph.validation
        );
        let iss = outlier.unwrap();
        assert_eq!(iss.severity, TopologySeverity::Warning);
        assert!(iss.cable.contains("E4.4") && iss.cable.contains("M4.2"));
    }

    #[test]
    fn wiring_outlier_via_cable_not_flagged() {
        let graph = build(
            "[p2b8]\n[src]\n    output = _WIRE\n    src = E4.4\n[sink]\n    input = _WIRE\n    dst = M4.2\n",
            &[],
        );
        let has_outlier = graph
            .validation
            .iter()
            .any(|iss| iss.message.contains("wiring outlier"));
        assert!(
            !has_outlier,
            "via-cable E4.4->M4.2 must NOT be flagged, got {:?}",
            graph.validation
        );
    }

    #[test]
    fn wiring_outlier_adjacent_not_flagged() {
        let graph = build("[p2b8]\n[copy]\n    a = B1.17\n    b = B1.18\n", &[]);
        let has_outlier = graph
            .validation
            .iter()
            .any(|iss| iss.message.contains("wiring outlier"));
        assert!(!has_outlier, "adjacent B1.17->B1.18 must NOT be flagged");
    }

    #[test]
    fn wiring_outlier_co_located_led_button_not_flagged() {
        let graph = build("[p2b8]\n[copy]\n    a = L1.17\n    b = B1.17\n", &[]);
        let has_outlier = graph
            .validation
            .iter()
            .any(|iss| iss.message.contains("wiring outlier"));
        assert!(!has_outlier, "co-located L1.17->B1.17 must NOT be flagged");
    }

    #[test]
    fn with_highlights_copies_influenced_sets_exactly() {
        let content =
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[sink]\n    input = _B\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());
        let sub = patch.influence_subtree(&[String::from("_A")]);
        let hl = graph.with_highlights(&sub);
        assert_eq!(hl.highlighted_nodes, sub.influenced_nodes);
        assert_eq!(hl.highlighted_edges, sub.influenced_edges);
        // original unchanged
        assert!(graph.highlighted_nodes.is_empty());
        assert!(graph.highlighted_edges.is_empty());
        // highlighted graph keeps nodes/edges/clusters/validation
        assert_eq!(hl.nodes, graph.nodes);
        assert_eq!(hl.edges, graph.edges);
    }

    #[test]
    fn with_highlights_empty_subtree_clears_highlights() {
        let content = "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());
        let empty = crate::patch::InfluenceSubtree::default();
        let hl = graph.with_highlights(&empty);
        assert!(hl.highlighted_nodes.is_empty());
        assert!(hl.highlighted_edges.is_empty());
        assert_eq!(hl.nodes, graph.nodes);
        assert_eq!(hl.edges, graph.edges);
    }

    #[test]
    fn fixture_highlight_sets() {
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
        let graph = Graph::build_from_patch(
            &patch,
            &clusters,
            &CostModel::default(),
            &GraphOptions::default(),
        );
        let vars = patch.hw_token_to_vars("B1.1");
        // B1.1 drives _TRIG and _EXTRA → at least those two cables in union
        assert!(vars.contains(&String::from("_TRIG")));
        assert!(vars.contains(&String::from("_EXTRA")));
        let sub = patch.influence_subtree(&vars);
        let hl = graph.with_highlights(&sub);
        assert_eq!(hl.highlighted_nodes, sub.influenced_nodes);
        assert_eq!(hl.highlighted_edges, sub.influenced_edges);
        // switch passthrough cable present
        assert!(sub.influenced_edges.contains("_SWOUT"));
        // copy chain cables present
        assert!(sub.influenced_edges.contains("_COPY1"));
        assert!(sub.influenced_edges.contains("_COPY2"));
    }

    #[test]
    fn latency_populated_parallel_to_edges_after_build() {
        let graph = build(
            "[p2b8]\n\
             [src]\n    output = _CLK\n\
             [sink1]\n    input = _CLK\n\
             [sink2]\n    input = _CLK\n\
             [sink3]\n    input = _CLK\n",
            &[],
        );
        let latency = graph
            .latency
            .expect("graph with cables must carry latency data");
        // One EdgeLatency per edge, indexed back into graph.edges in order.
        assert_eq!(latency.edges.len(), graph.edges.len());
        for (i, l) in latency.edges.iter().enumerate() {
            assert_eq!(l.edge_index, i, "edge {i} latency must index graph.edges");
        }
        // src (section 1) fans out to sinks at sections 2..4: all forward.
        // The synthetic circuits are unknown to the schema, so AVG falls back
        // to unit cost and latencies are 1×, 2×, 3× one loop step.
        assert_eq!(latency.summary.back_edge_count, 0);
        assert_eq!(latency.summary.max, 3.0);
        assert_eq!(latency.summary.avg, 2.0);
    }

    #[test]
    fn latency_summary_counts_mixed_forward_and_back_edges() {
        // Three forward edges (1 step each) plus one back-edge wrapping the
        // loop: _BACK is produced by [b] (section 3) and consumed by [e]
        // (section 1), so L = ((1 − 3) mod 6) × AVG = 4 × unit cost.
        let graph = build(
            "[p2b8]\n\
             [e]\n    input = _BACK\n\
             [a]\n    output = _A\n\
             [b]\n    input = _A\n    output = _BACK\n    output = _B\n\
             [c]\n    input = _B\n    output = _C\n\
             [d]\n    input = _C\n",
            &[],
        );
        let latency = graph
            .latency
            .expect("graph with cables must carry latency data");
        assert_eq!(latency.edges.len(), 4);
        let back: Vec<_> = latency.edges.iter().filter(|l| l.is_back_edge).collect();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].latency, 4.0);
        assert_eq!(latency.summary.back_edge_count, 1);
        assert_eq!(latency.summary.max, 4.0);
        assert_eq!(latency.summary.avg, (4.0 + 1.0 + 1.0 + 1.0) / 4.0);
    }

    #[test]
    fn latency_none_for_degenerate_or_empty_graphs() {
        // No cables → no edges → nothing to measure.
        let graph = build("[p2b8]\n[copy]\n", &[]);
        assert_eq!(graph.latency, None);
        // The hand-built default Graph carries no latency either.
        assert_eq!(Graph::default().latency, None);
    }

    #[test]
    fn with_highlights_keeps_latency() {
        let content =
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[sink]\n    input = _B\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());
        assert!(graph.latency.is_some());
        let sub = patch.influence_subtree(&[String::from("_A")]);
        // with_highlights keeps the full graph's topology, so latency survives.
        assert_eq!(graph.with_highlights(&sub).latency, graph.latency);
    }

    #[test]
    fn register_edges_build_nodes_and_edges() {
        let schema = catalog_test_schema();
        crate::schema::set_test_schema(Some(schema));
        let patch = Patch::from_ini_file(std::path::Path::new("fixtures/graph_register_edges.ini"))
            .unwrap();
        let graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());
        crate::schema::reset_test_schema();

        // --- Nodes: 9 circuit nodes + 1 controller node + 3 jack nodes ---
        let circuit_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Circuit)
            .collect();
        assert_eq!(circuit_nodes.len(), 9);
        let ctrl_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Controller)
            .collect();
        assert_eq!(ctrl_nodes.len(), 1);
        assert_eq!(
            ctrl_nodes[0].id,
            NodeId::Controller(String::from("p2b8"), 1)
        );
        // L1.1 resolves to the P2B8 controller (L is in PBESLR and ordinal 1
        // matches), so only O1, I1, and B2.1 create jack nodes.
        let jack_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind != NodeKind::Circuit && n.kind != NodeKind::Controller)
            .collect();
        assert_eq!(jack_nodes.len(), 3);
        let jack_ids: HashSet<&NodeId> = jack_nodes.iter().map(|n| &n.id).collect();
        assert!(jack_ids.contains(&NodeId::Jack(String::from("O1"))));
        assert!(jack_ids.contains(&NodeId::Jack(String::from("I1"))));
        // B2.1 is a PBESLR letter with no declared controller ordinal 2, so it
        // falls back to an input jack on the master.
        assert!(jack_ids.contains(&NodeId::Jack(String::from("B2.1"))));
        assert_eq!(
            jack_nodes
                .iter()
                .find(|n| n.id == NodeId::Jack(String::from("I1")))
                .unwrap()
                .kind,
            NodeKind::InputJack
        );

        // --- Register edges (cable prefix "_REG:") ---
        let reg_edges: Vec<&GraphEdge> = graph
            .edges
            .iter()
            .filter(|e| e.cable.starts_with("_REG:"))
            .collect();
        // Expected register edges:
        //   buttonwriter -> P2B8  (B1.1, output)
        //   P2B8 -> potreader     (P1.1, input)
        //   jackwriter -> O1      (O1, output)
        //   jacksharer -> O1      (O1, via cv key fallback)
        //   unknowncircuit_foobar -> P2B8  (L1.1, fallback led heuristic)
        //   I1 -> jackreader      (I1, input)
        //   B2.1 -> b2fallback    (B2.1, unmatched ordinal → input jack)
        assert_eq!(reg_edges.len(), 7);

        // Controller write: buttonwriter -> P2B8
        let btn_edge = reg_edges
            .iter()
            .find(|e| e.cable == "_REG:B1.1")
            .expect("_REG:B1.1 edge");
        assert_eq!(btn_edge.source, NodeId::circuit("buttonwriter", 0));
        assert_eq!(btn_edge.sink, NodeId::Controller(String::from("p2b8"), 1));

        // Controller read: P2B8 -> potreader
        let pot_edge = reg_edges
            .iter()
            .find(|e| e.cable == "_REG:P1.1")
            .expect("_REG:P1.1 edge");
        assert_eq!(pot_edge.source, NodeId::Controller(String::from("p2b8"), 1));
        assert_eq!(pot_edge.sink, NodeId::circuit("potreader", 0));

        // Output jack: jackwriter -> O1
        let ow_edge = reg_edges
            .iter()
            .find(|e| e.cable == "_REG:O1")
            .expect("_REG:O1 edge");
        assert_eq!(ow_edge.source, NodeId::circuit("jackwriter", 0));
        assert_eq!(ow_edge.sink, NodeId::Jack(String::from("O1")));

        // Shared jack node: jacksharer reads from O1 (cv key → input fallback)
        let sh_edge = reg_edges
            .iter()
            .filter(|e| e.cable == "_REG:O1")
            .find(|e| e.sink == NodeId::circuit("jacksharer", 0))
            .expect("O1 -> jacksharer edge");
        assert_eq!(sh_edge.source, NodeId::Jack(String::from("O1")));

        // Fallback output heuristic: unknowncircuit_foobar -> P2B8 (led key)
        let led_edge = reg_edges
            .iter()
            .find(|e| e.cable == "_REG:L1.1")
            .expect("_REG:L1.1 edge");
        assert_eq!(led_edge.source, NodeId::circuit("unknowncircuit_foobar", 0));
        assert_eq!(led_edge.sink, NodeId::Controller(String::from("p2b8"), 1));

        // Input jack: I1 -> jackreader
        let ij_edge = reg_edges
            .iter()
            .find(|e| e.cable == "_REG:I1")
            .expect("_REG:I1 edge");
        assert_eq!(ij_edge.source, NodeId::Jack(String::from("I1")));
        assert_eq!(ij_edge.sink, NodeId::circuit("jackreader", 0));

        // Unmatched-ordinal fallback: B2.1 has no declared controller ordinal
        // 2, so the button ref lands on an input-jack node; the unknown
        // circuit reads it (button is not an output/led key).
        let b2_edge = reg_edges
            .iter()
            .find(|e| e.cable == "_REG:B2.1")
            .expect("_REG:B2.1 edge");
        assert_eq!(b2_edge.source, NodeId::Jack(String::from("B2.1")));
        assert_eq!(b2_edge.sink, NodeId::circuit("b2fallback", 0));

        // Total edges: 5 cable edges + 7 register edges = 12
        assert_eq!(graph.edges.len(), 12);
    }
    fn dep_node(id: NodeId, kind: NodeKind) -> GraphNode {
        GraphNode {
            id,
            kind,
            circuit: String::new(),
            instance_index: 0,
            section_index: 0,
        }
    }

    fn dep_edge(cable: &str, source: NodeId, sink: NodeId) -> GraphEdge {
        GraphEdge {
            cable: cable.to_string(),
            source,
            sink,
        }
    }

    /// 0 controller, 1 ledw, 2 feed, 3 root, 4 other (writes the controller's
    /// LED), 5 clk, 6 c (cycle partner of root).
    fn dependency_graph() -> Graph {
        let nodes = vec![
            dep_node(
                NodeId::Controller(String::from("p2b8"), 1),
                NodeKind::Controller,
            ),
            dep_node(NodeId::circuit("ledw", 0), NodeKind::Circuit),
            dep_node(NodeId::circuit("feed", 0), NodeKind::Circuit),
            dep_node(NodeId::circuit("root", 0), NodeKind::Circuit),
            dep_node(NodeId::circuit("other", 0), NodeKind::Circuit),
            dep_node(NodeId::circuit("clk", 0), NodeKind::Circuit),
            dep_node(NodeId::circuit("c", 0), NodeKind::Circuit),
        ];
        let edges = vec![
            dep_edge("_g", NodeId::circuit("ledw", 0), NodeId::circuit("root", 0)),
            dep_edge("_h", NodeId::circuit("clk", 0), NodeId::circuit("root", 0)),
            dep_edge("_f", NodeId::circuit("feed", 0), NodeId::circuit("ledw", 0)),
            dep_edge(
                "_REG:B1.1",
                NodeId::Controller(String::from("p2b8"), 1),
                NodeId::circuit("ledw", 0),
            ),
            dep_edge(
                "_REG:L1.1",
                NodeId::circuit("other", 0),
                NodeId::Controller(String::from("p2b8"), 1),
            ),
            dep_edge("_r", NodeId::circuit("root", 0), NodeId::circuit("c", 0)),
            dep_edge("_c", NodeId::circuit("c", 0), NodeId::circuit("root", 0)),
        ];
        Graph {
            nodes,
            edges,
            clusters: Vec::new(),
            validation: Vec::new(),
            latency: None,
            highlighted_nodes: HashSet::new(),
            highlighted_edges: HashSet::new(),
            not_selected: HashSet::new(),
        }
    }

    fn deps_of(graph: &Graph, root: &NodeId) -> Vec<NodeId> {
        graph
            .upstream_dependencies(root)
            .iter()
            .map(|&i| graph.nodes[i].id.clone())
            .collect()
    }

    /// The walk covers a linear chain (feed → ledw → root), a fork (ledw and
    /// clk both feed root), and a cycle (root ↔ c) without revisiting nodes.
    #[test]
    fn upstream_dependencies_chain_fork_and_cycle() {
        let graph = dependency_graph();
        let deps = deps_of(&graph, &NodeId::circuit("root", 0));
        // BFS order: root, then its producers ledw/clk/c, then their upstream.
        assert_eq!(deps[0], NodeId::circuit("root", 0));
        let set: HashSet<NodeId> = deps.iter().cloned().collect();
        assert_eq!(
            set,
            [
                NodeId::circuit("root", 0),
                NodeId::circuit("ledw", 0),
                NodeId::circuit("clk", 0),
                NodeId::circuit("c", 0),
                NodeId::circuit("feed", 0),
                NodeId::Controller(String::from("p2b8"), 1),
            ]
            .into_iter()
            .collect()
        );
        // Cycle: each of root and c appears exactly once.
        assert_eq!(
            deps.iter()
                .filter(|n| **n == NodeId::circuit("root", 0))
                .count(),
            1
        );
        assert_eq!(
            deps.iter()
                .filter(|n| **n == NodeId::circuit("c", 0))
                .count(),
            1
        );
    }

    /// The walk stops at the controller: its incoming LED-write edge from
    /// `other` is not followed, so `other` stays out of the dependency set.
    #[test]
    fn upstream_dependencies_stops_at_controller_leaf() {
        let graph = dependency_graph();
        let deps = deps_of(&graph, &NodeId::circuit("root", 0));
        assert!(!deps.contains(&NodeId::circuit("other", 0)));
        // The controller itself is reached (it feeds ledw via B1.1) and is a leaf.
        assert!(deps.contains(&NodeId::Controller(String::from("p2b8"), 1)));
    }

    /// A controller or input-jack root is a leaf: the walk yields only it.
    #[test]
    fn upstream_dependencies_root_is_leaf() {
        let graph = dependency_graph();
        let controller = NodeId::Controller(String::from("p2b8"), 1);
        assert_eq!(deps_of(&graph, &controller), vec![controller.clone()]);

        let mut jack_graph = dependency_graph();
        jack_graph.nodes.push(dep_node(
            NodeId::Jack(String::from("I1")),
            NodeKind::InputJack,
        ));
        let jack = NodeId::Jack(String::from("I1"));
        assert_eq!(deps_of(&jack_graph, &jack), vec![jack]);
    }

    /// An output-jack root is NOT a leaf: its producers are walked (the
    /// typical dependency cut starts at an output jack).
    #[test]
    fn upstream_dependencies_output_jack_root_walks_producers() {
        let mut graph = dependency_graph();
        // producer → O1: the jack's incoming edge is a dependency.
        graph.nodes.push(dep_node(
            NodeId::Jack(String::from("O1")),
            NodeKind::OutputJack,
        ));
        graph.edges.push(dep_edge(
            "_REG:O1",
            NodeId::circuit("root", 0),
            NodeId::Jack(String::from("O1")),
        ));
        let deps = deps_of(&graph, &NodeId::Jack(String::from("O1")));
        assert!(deps.contains(&NodeId::Jack(String::from("O1"))));
        assert!(deps.contains(&NodeId::circuit("root", 0)));
        assert!(deps.contains(&NodeId::circuit("ledw", 0)));
        assert!(!deps.contains(&NodeId::circuit("other", 0)));
    }

    /// An unknown root yields nothing.
    #[test]
    fn upstream_dependencies_unknown_root_yields_nothing() {
        let graph = dependency_graph();
        assert!(graph
            .upstream_dependencies(&NodeId::circuit("ghost", 0))
            .is_empty());
    }

    /// `internal_edges` returns exactly the edges whose endpoints are both in
    /// the member set.
    #[test]
    fn internal_edges_keeps_only_edges_within_members() {
        let graph = dependency_graph();
        let members: HashSet<usize> = graph
            .upstream_dependencies(&NodeId::circuit("root", 0))
            .into_iter()
            .collect();
        let cables: HashSet<&str> = graph
            .internal_edges(&members)
            .iter()
            .map(|&i| graph.edges[i].cable.as_str())
            .collect();
        // feed -> ledw is internal; the LED write into the controller is not
        // (the controller is a member, `other` is not).
        assert_eq!(
            cables,
            ["_g", "_h", "_f", "_REG:B1.1", "_r", "_c"]
                .into_iter()
                .collect()
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
        Graph::build_from_patch(
            &patch,
            &clusters,
            &CostModel::default(),
            &GraphOptions::default(),
        )
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
        let circuit_nodes: Vec<&GraphNode> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Circuit)
            .collect();
        assert_eq!(circuit_nodes.len(), 14);

        let buttons: Vec<&GraphNode> = circuit_nodes
            .iter()
            .filter(|n| n.circuit == "button")
            .copied()
            .collect();
        for (i, b) in buttons.iter().enumerate() {
            assert_eq!(b.instance_index, i);
            assert_eq!(b.id, NodeId::circuit("button", i));
        }

        let copies: Vec<&GraphNode> = circuit_nodes
            .iter()
            .filter(|n| n.circuit == "copy")
            .copied()
            .collect();
        assert_eq!(copies[0].instance_index, 0);
        assert_eq!(copies[1].instance_index, 1);
        assert_ne!(copies[0].id, copies[1].id);

        // Canonical circuit set (single occurrence of each distinct name).

        // Canonical circuit set (single occurrence of each distinct name).
        let mut circuits: Vec<&str> = circuit_nodes.iter().map(|n| n.circuit.as_str()).collect();
        circuits.sort();
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
        let cable_edges: Vec<&GraphEdge> = graph
            .edges
            .iter()
            .filter(|e| !e.cable.starts_with("_REG:"))
            .collect();
        assert_eq!(cable_edges.len(), 11);

        for e in &cable_edges {
            assert_eq!(
                e.source,
                NodeId::circuit("button", 0),
                "cable {} must be produced by the first button instance",
                e.cable
            );
            assert_eq!(
                e.sink,
                NodeId::circuit("arpeggio", 0),
                "cable {} must be consumed by the arpeggio section",
                e.cable
            );
        }

        // _SCALE reaches four arpeggio select params (fan-out within the patch).
        let scale_edges: Vec<&GraphEdge> = cable_edges
            .iter()
            .filter(|e| e.cable == "_SCALE")
            .copied()
            .collect();
        assert_eq!(scale_edges.len(), 4);
        for e in &scale_edges {
            assert_eq!(e.sink, NodeId::circuit("arpeggio", 0));
        }
    }

    #[test]
    fn alg27_model_shape_matches_164_sections_with_repeated_instances() {
        // 164 sections across 22 distinct circuit names; repeated names get
        // unique ids rather than colliding.
        let graph = fixture_graph("alg27_2.ini");
        let circuit_nodes: Vec<&GraphNode> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Circuit)
            .collect();
        assert_eq!(circuit_nodes.len(), 164);

        let mut section_indices: Vec<usize> =
            circuit_nodes.iter().map(|n| n.section_index).collect();
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
            assert_eq!(e.source, NodeId::circuit("clocktool", 0));
        }
        let sinks: std::collections::HashSet<&NodeId> = clk.iter().map(|e| &e.sink).collect();
        assert_eq!(sinks.len(), 2, "sinks resolve to copy and clocktool only");
        assert!(sinks.contains(&NodeId::circuit("clocktool", 0)));
        assert!(sinks.contains(&NodeId::circuit("copy", 0)));
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
            assert_eq!(e.source, NodeId::circuit("pot", 0));
            assert_eq!(e.sink, NodeId::circuit("matrixmixer", 0));
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
        let graph = Graph::build_from_patch(
            &patch,
            &clusters,
            &CostModel::default(),
            &GraphOptions::default(),
        );

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
        // are not flagged by the topology (dangling/n→1) channel. `_MATRIXSEL`
        // additionally carries the per-token influence second opinion (design
        // D4): its root var pot P3.2 reaches 7 circuits, an extreme outlier for
        // kind P (mean 0.055, std 0.76) — a Warning that never gates loading.
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
        assert_eq!(by_cable.get("_MATRIXEDIT"), None);
        // _MATRIXSEL: the dangling channel stays silent, the influence
        // second opinion fires (P3.2 → _MATRIXSEL, z≈9.1 > 3.0 band).
        let matr = graph.validation.iter().find(|i| i.cable == "_MATRIXSEL");
        assert!(
            matr.is_some() && matr.unwrap().message.contains("influence outlier"),
            "_MATRIXSEL carries the P3.2 influence-outlier finding"
        );
        assert_eq!(matr.unwrap().severity, TopologySeverity::Warning);
    }

    #[test]
    fn influence_outlier_extreme_token_flagged() {
        // Spec scenario: a hardware token whose influence subtree is an extreme
        // outlier for its token kind → the associated cable renders with the
        // error-highlight token and the finding is a Warning. A button driving
        // 24 fan-out circuits is far beyond the B-kind corpus band (mean 1.5,
        // std 7.1 → z ≈ 3.2).
        let mut content = String::from(
            "[p2b8]\n\
             [button]\n    button = B1.1\n    output = _A\n",
        );
        for i in 0..24 {
            content.push_str(&format!("[copy]\n    input = _A\n    output = _OUT{i}\n"));
        }
        let patch = Patch::from_ini_str(&content, String::from("extreme")).unwrap();
        let graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());
        let z = patch
            .token_influence_z_score("B1.1")
            .expect("B-kind has stats");
        assert!(
            z > 3.0,
            "button driving 24 circuits must exceed the z-band, got {z:.2}"
        );
        let finding = graph
            .validation
            .iter()
            .find(|i| i.message.contains("influence outlier"));
        assert!(
            finding.is_some(),
            "extreme-token patch must produce an influence warning"
        );
        assert_eq!(finding.unwrap().severity, TopologySeverity::Warning);
        assert!(finding.unwrap().cable.contains("_A"));
    }

    #[test]
    fn influence_outlier_typical_token_not_flagged() {
        // Spec scenario: every token within the calibrated band → no additional
        // topology warning. A button driving one copy circuit stays near the
        // B-kind mean (z ≈ 0).
        let content = "[p2b8]\n\
             [button]\n    button = B1.1\n    output = _A\n\
             [copy]\n    input = _A\n    output = _OUT\n";
        let patch = Patch::from_ini_str(content, String::from("typical")).unwrap();
        let graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());
        let z = patch
            .token_influence_z_score("B1.1")
            .expect("B-kind has stats");
        assert!(z <= 3.0, "typical button must stay in band, got {z:.2}");
        assert!(
            !graph
                .validation
                .iter()
                .any(|i| i.message.contains("influence outlier")),
            "typical patch must not produce an influence warning"
        );
    }

    #[test]
    fn select_state_classification_matrix() {
        let content = "\
[p2b8]\n\
[foo]\n    button = B1.1\n\
[bar]\n    select = S1.1\n    selectat = 0\n    button = B1.1\n\
[baz]\n    select = _CABLE\n    button = B1.1\n\
[qux]\n    select = _A + 1\n    button = B1.1\n\
[quux]\n    select = S9.9\n    button = B1.1\n";
        let patch = Patch::from_ini_str(content, String::from("matrix")).unwrap();
        // foo has no select => Selected regardless of state
        let opts_empty = GraphOptions::default();
        let g0 = Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_empty);
        assert_eq!(g0.nodes.iter().filter(|n| n.circuit == "foo").count(), 1);
        // bar select S1.1 with no state => Unknown (keeps edges, not filtered)
        let unknown_graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_empty);
        // bar should keep controller edge when Unknown
        let bar_ctrl = unknown_graph
            .edges
            .iter()
            .any(|e| e.cable == "_REG:B1.1" && e.sink == NodeId::circuit("bar", 0));
        assert!(bar_ctrl, "Unknown keeps controller edge");
        // bar with matching state => Selected keeps controller edge
        let mut state_match = HashMap::new();
        state_match.insert(String::from("S1.1"), 0.0);
        let opts_match = GraphOptions {
            state: state_match.clone(),
            hide_unselected: false,
        };
        let g_match = Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_match);
        let bar_match = g_match
            .edges
            .iter()
            .any(|e| e.cable == "_REG:B1.1" && e.sink == NodeId::circuit("bar", 0));
        assert!(bar_match, "Selected keeps controller edge");
        // bar with mismatched state => NotSelected drops controller edge
        // bar has select=S1.1 selectat=0, so state S1.1=1 mismatches selectat 0 => NotSelected
        let mut state_mismatch = HashMap::new();
        state_mismatch.insert(String::from("S1.1"), 1.0);
        let opts_mismatch = GraphOptions {
            state: state_mismatch,
            hide_unselected: false,
        };
        let g_mismatch =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_mismatch);
        let bar_mismatch = g_mismatch
            .edges
            .iter()
            .any(|e| e.cable == "_REG:B1.1" && e.sink == NodeId::circuit("bar", 0));
        assert!(
            !bar_mismatch,
            "NotSelected drops controller edge (selectat 0 vs S1.1=1)"
        );
        // baz select _CABLE with matching state
        let mut cable_state = HashMap::new();
        cable_state.insert(String::from("_CABLE"), 2.5);
        let opts_cable = GraphOptions {
            state: cable_state,
            hide_unselected: false,
        };
        let g_cable = Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_cable);
        let baz_keep = g_cable
            .edges
            .iter()
            .any(|e| e.cable == "_REG:B1.1" && e.sink == NodeId::circuit("baz", 0));
        assert!(baz_keep, "Selected _CABLE keeps controller");
        // qux select is non-single-token => Unknown, never NotSelected even with state
        let mut state_q = HashMap::new();
        state_q.insert(String::from("_A"), 0.0);
        let opts_q = GraphOptions {
            state: state_q,
            hide_unselected: false,
        };
        let g_q = Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_q);
        let qux_edge = g_q
            .edges
            .iter()
            .any(|e| e.cable == "_REG:B1.1" && e.sink == NodeId::circuit("qux", 0));
        assert!(qux_edge, "complex expression is Unknown, keeps edge");
        // quux select S9.9 not in state => Unknown keeps edge
        let g_quux = Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_q);
        let quux_edge = g_quux
            .edges
            .iter()
            .any(|e| e.cable == "_REG:B1.1" && e.sink == NodeId::circuit("quux", 0));
        assert!(quux_edge, "root not in state => Unknown keeps edge");
    }

    #[test]
    fn not_selected_keeps_cable_and_jack_edges() {
        let content = "\
[p2b8]\n\
[foo]\n    select = S1.1\n    selectat = 0\n    button = B1.1\n    input = _FOO\n    cv_in = I1\n    cv_out = O1\n\
[bar]\n    output = _FOO\n";
        let patch = Patch::from_ini_str(content, String::from("keep")).unwrap();
        let mut state = HashMap::new();
        state.insert(String::from("S1.1"), 1.0);
        let opts = GraphOptions {
            state,
            hide_unselected: false,
        };
        let graph = Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts);
        // cable edge bar -> foo must remain
        assert!(
            graph.edges.iter().any(|e| e.cable == "_FOO"),
            "NotSelected keeps cable edge"
        );
        // InputJack I1 -> foo must remain
        assert!(
            graph.edges.iter().any(|e| e.cable == "_REG:I1"),
            "NotSelected keeps InputJack edge"
        );
        // foo -> OutputJack O1 must remain
        assert!(
            graph.edges.iter().any(|e| e.cable == "_REG:O1"),
            "NotSelected keeps OutputJack edge"
        );
        // Controller edge B1.1 must be dropped
        let has_ctrl = graph.edges.iter().any(|e| {
            e.cable == "_REG:B1.1"
                && (e.source == NodeId::circuit("foo", 0) || e.sink == NodeId::circuit("foo", 0))
        });
        assert!(!has_ctrl, "NotSelected drops controller edge");
        // hide_unselected true drops the circuit node but keeps jacks/cable edge
        let mut state2 = HashMap::new();
        state2.insert(String::from("S1.1"), 1.0);
        let opts_hide = GraphOptions {
            state: state2,
            hide_unselected: true,
        };
        let graph_hide = Graph::build_from_patch(&patch, &[], &CostModel::default(), &opts_hide);
        assert!(
            !graph_hide
                .nodes
                .iter()
                .any(|n| n.id == NodeId::circuit("foo", 0)),
            "hide_unselected drops NotSelected circuit node"
        );
        assert!(
            graph_hide
                .nodes
                .iter()
                .any(|n| n.id == NodeId::circuit("bar", 0)),
            "Selected nodes remain"
        );
        assert!(
            graph_hide
                .nodes
                .iter()
                .any(|n| n.id == NodeId::Jack(String::from("I1"))),
            "jack nodes survive hide"
        );
        assert!(
            graph_hide.edges.iter().any(|e| e.cable == "_FOO"),
            "cable edge survives hide"
        );
    }

    #[test]
    fn default_graph_is_byte_identical_to_unassumed() {
        let content = "\
[p2b8]\n\
[foo]\n    button = B1.1\n    output = _A\n\
[bar]\n    input = _A\n    cv_in = I1\n";
        let patch = Patch::from_ini_str(content, String::from("identical")).unwrap();
        let g_default =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());
        let g_empty_state = Graph::build_from_patch(
            &patch,
            &[],
            &CostModel::default(),
            &GraphOptions {
                state: HashMap::new(),
                hide_unselected: false,
            },
        );
        assert_eq!(g_default.nodes, g_empty_state.nodes);
        assert_eq!(g_default.edges, g_empty_state.edges);
        assert_eq!(g_default.validation, g_empty_state.validation);
        // also check that hide false with empty state equals old no-options build
        let g_hide_false = Graph::build_from_patch(
            &patch,
            &[],
            &CostModel::default(),
            &GraphOptions {
                state: HashMap::new(),
                hide_unselected: false,
            },
        );
        assert_eq!(g_default.edges.len(), g_hide_false.edges.len());
    }

    #[test]
    fn select_state_fixture_drops_losing_controller_edges() {
        // fixtures/graph_select_state.ini: sel_a (selectat 0) and sel_b
        // (selectat 1) both select on S7.1; a button circuit carries no
        // select and an outputjack drives O1. Assuming each S7.1 value drops
        // the losing circuit's controller edge while its cable and jack
        // edges survive.
        let patch = Patch::from_ini_file(Path::new("fixtures/graph_select_state.ini")).unwrap();
        let clusters: Vec<Cluster> = patch
            .banner_groups
            .iter()
            .map(|g| Cluster {
                title: g.banner.clone().unwrap_or_default(),
                section_range: g.section_range.clone(),
            })
            .collect();

        // S7.1 = 0: sel_b (selectat 1) is NotSelected.
        let opts0 = GraphOptions {
            state: HashMap::from([(String::from("S7.1"), 0.0)]),
            hide_unselected: false,
        };
        let g0 = Graph::build_from_patch(&patch, &clusters, &CostModel::default(), &opts0);
        assert!(
            g0.not_selected.contains(&3),
            "sel_b (section 3) is NotSelected under S7.1=0"
        );
        assert!(
            !g0.edges.iter().any(|e| e.cable == "_REG:B2.1"),
            "S7.1=0 drops sel_b's controller edge"
        );
        assert!(
            g0.edges.iter().any(|e| e.cable == "_REG:B1.1"),
            "S7.1=0 keeps sel_a's controller edge"
        );
        assert!(
            g0.edges.iter().any(|e| e.cable == "_SELB"),
            "S7.1=0 keeps sel_b's cable edge"
        );
        assert!(
            g0.edges.iter().any(|e| e.cable == "_REG:S7.1"),
            "S7.1=0 keeps the select jack edge"
        );
        assert!(
            g0.edges.iter().any(|e| e.cable == "_REG:O1"),
            "S7.1=0 keeps the output jack edge"
        );

        // S7.1 = 1: sel_a (selectat 0) is NotSelected.
        let opts1 = GraphOptions {
            state: HashMap::from([(String::from("S7.1"), 1.0)]),
            hide_unselected: false,
        };
        let g1 = Graph::build_from_patch(&patch, &clusters, &CostModel::default(), &opts1);
        assert!(
            g1.not_selected.contains(&2),
            "sel_a (section 2) is NotSelected under S7.1=1"
        );
        assert!(
            !g1.edges.iter().any(|e| e.cable == "_REG:B1.1"),
            "S7.1=1 drops sel_a's controller edge"
        );
        assert!(
            g1.edges.iter().any(|e| e.cable == "_REG:B2.1"),
            "S7.1=1 keeps sel_b's controller edge"
        );
        assert!(
            g1.edges.iter().any(|e| e.cable == "_SELA"),
            "S7.1=1 keeps sel_a's cable edge"
        );
        assert!(
            g1.edges.iter().any(|e| e.cable == "_REG:O1"),
            "S7.1=1 keeps the output jack edge"
        );
    }

    #[test]
    fn select_state_fixture_default_build_is_stable() {
        // No assumed state: the unassumed build is byte-identical to the
        // pre-select-state build (nothing gated, nothing dropped), and both
        // controller edges stay present.
        let patch = Patch::from_ini_file(Path::new("fixtures/graph_select_state.ini")).unwrap();
        let clusters: Vec<Cluster> = patch
            .banner_groups
            .iter()
            .map(|g| Cluster {
                title: g.banner.clone().unwrap_or_default(),
                section_range: g.section_range.clone(),
            })
            .collect();
        let g_default = Graph::build_from_patch(
            &patch,
            &clusters,
            &CostModel::default(),
            &GraphOptions::default(),
        );
        let g_empty = Graph::build_from_patch(
            &patch,
            &clusters,
            &CostModel::default(),
            &GraphOptions {
                state: HashMap::new(),
                hide_unselected: false,
            },
        );
        assert!(g_default.not_selected.is_empty());
        assert_eq!(g_default.nodes, g_empty.nodes);
        assert_eq!(g_default.edges, g_empty.edges);
        assert_eq!(g_default.validation, g_empty.validation);
        assert!(g_default.edges.iter().any(|e| e.cable == "_REG:B1.1"));
        assert!(g_default.edges.iter().any(|e| e.cable == "_REG:B2.1"));
    }

    /// Build the dependency-walk fixture's graph with default options.
    fn dependency_walk_graph() -> Graph {
        let patch = Patch::from_ini_file(Path::new("fixtures/graph_dependency_walk.ini")).unwrap();
        Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default())
    }

    /// The O1 output jack's dependency set covers its two producers (one via
    /// the _mix cable, one direct) and the controllers feeding their buttons,
    /// while the unrelated circuit and the LED-write branch stay cut at the
    /// controller (task D 4.1).
    #[test]
    fn dependency_walk_fixture_covers_producers_and_stops_at_controller() {
        let graph = dependency_walk_graph();
        let deps: HashSet<NodeId> = graph
            .upstream_dependencies(&NodeId::Jack(String::from("O1")))
            .into_iter()
            .map(|i| graph.nodes[i].id.clone())
            .collect();
        let expected: HashSet<NodeId> = [
            NodeId::Jack(String::from("O1")),
            NodeId::circuit("mixer", 0),
            NodeId::circuit("producer_a", 0),
            NodeId::circuit("producer_b", 0),
            NodeId::Controller(String::from("p2b8"), 1),
            NodeId::Controller(String::from("p2b8"), 2),
        ]
        .into_iter()
        .collect();
        assert_eq!(deps, expected, "O1 dependency set mismatch");
        // The unrelated circuit and the LED-write branch are excluded: the
        // walk stops at the controller and never follows its incoming LED edge.
        assert!(!deps.contains(&NodeId::circuit("unrelated", 0)));
        assert!(!deps.contains(&NodeId::circuit("led_writer", 0)));
    }

    /// The dependency set's internal edges are exactly the edges with both
    /// endpoints in the set (task D 4.1): the _mix cable, the two _REG:O1
    /// edges, and the two button-feedback register edges.
    #[test]
    fn dependency_walk_fixture_internal_edges_are_within_members() {
        let graph = dependency_walk_graph();
        let members: HashSet<usize> = graph
            .upstream_dependencies(&NodeId::Jack(String::from("O1")))
            .into_iter()
            .collect();
        let cables: HashSet<String> = graph
            .internal_edges(&members)
            .iter()
            .map(|&i| graph.edges[i].cable.clone())
            .collect();
        let expected: HashSet<String> = [
            String::from("_mix"),
            String::from("_REG:O1"),
            String::from("_REG:B1.1"),
            String::from("_REG:B2.1"),
        ]
        .into_iter()
        .collect();
        assert_eq!(cables, expected, "internal edge set mismatch");
    }
}
