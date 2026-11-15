//! Same-type node grouping: a post-solve view transform (graph-same-type-grouping
//! design D1).
//!
//! Grouping never re-arranges anything. It consumes the display positions the
//! solver already published plus the per-node world extents (ADR 40) and answers
//! one question — which drawn node stands in for which model node — so the
//! painter, the pointer hit test and the highlight mapping all read the same
//! answer instead of each deriving its own.
//!
//! Pure and deterministic: union-find merges in ascending node index and every
//! derived collection is sorted, so the same graph, positions and settings
//! always produce an identical [`GroupedView`] (design D2). Because the module
//! is pure it carries no terminal, egui or winit dependency and is testable
//! headless.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::BuildHasher;

use crate::graph::{Graph, NodeId, NodeKind};
use crate::layout::NODE_WORLD_H;

/// Cable hops along which same-type circuits still merge (design D2).
///
/// Two hops catches a short same-type cable chain that the drawn layout does not
/// put next to each other; a larger budget starts collapsing genuinely distant
/// parts of a patch. A module constant rather than a setting, because only a
/// real need should promote it (design D6).
pub const GROUP_HOP_LIMIT: usize = 2;

/// Spatial merge slack, as a multiple of the mean node extent per axis.
///
/// A value of `1.5` merges nodes whose drawn bodies sit within half their mean
/// size of each other on *both* axes — a stacked run in one column qualifies,
/// diagonal neighbours across a gap and parallel but distant columns do not.
pub const GROUP_PROXIMITY_RATIO: f32 = 1.5;

/// Slack added to the proximity bound on each axis.
///
/// The column arrangement separates stacked nodes by exactly the body height, so
/// an un-padded bound would sit on the comparison boundary and any future change
/// to that spacing would silently stop a stack from grouping. The pad is small
/// enough that it cannot reach a second neighbour slot.
const PROXIMITY_EPSILON: f32 = 4.0;

/// Width assumed for a node whose entry is missing from the caller's `sizes`.
///
/// The height fallback is [`NODE_WORLD_H`] (the real body height), so only the
/// width — the per-node text estimate — needs a stand-in; this is roughly the
/// drawn width of a short circuit title with one port on each side.
const NOMINAL_WIDTH: f32 = 120.0;

/// Grouping knobs, mirrored from the `[graph]` config table (design D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupSettings {
    /// When false the view is [`GroupedView::identity`]: nothing is collapsed,
    /// which is how `Shift+O` makes grouping reversible at runtime.
    pub enabled: bool,
    /// Smallest component that still becomes a group. Below it every member is
    /// drawn individually, so a pair of same-type circuits is never hidden
    /// behind a box that costs more attention than it saves.
    pub min_size: usize,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            min_size: 5,
        }
    }
}

/// One collapsed group of same-type circuit nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The circuits' shared type, e.g. `"encoder"`; every member carries it.
    pub circuit: String,
    /// Original node indices, ascending — the order the label, the peek list
    /// and the isolate subgraph all read, and what makes them deterministic.
    pub members: Vec<usize>,
    /// Bounding-box centre of the members' drawn positions: where the collapsed
    /// box is drawn, so grouping needs no solve of its own (design D1).
    pub center: (f32, f32),
    /// `(min_x, min_y, max_x, max_y)` over the members' drawn positions.
    pub bbox: (f32, f32, f32, f32),
}

/// A drawn stand-in for one model node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewNode {
    /// Node `i` is drawn on its own: an ungrouped node, or the member of an
    /// expanded group.
    Individual(usize),
    /// Group `g` is drawn in place of this member, because `g` is collapsed.
    Group(usize),
}

/// One drawn cable, between drawn stand-ins rather than model nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewEdge {
    pub source: ViewNode,
    pub sink: ViewNode,
}

/// The grouping pass's whole answer: the groups, the node mapping and the
/// aggregated cables.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupedView {
    /// Groups ordered by ascending first member; index into this vector is the
    /// `usize` inside [`ViewNode::Group`].
    pub groups: Vec<Group>,
    /// Parallel to `graph.nodes`: what to draw for each model node.
    pub nodes: Vec<ViewNode>,
    /// Deduplicated cables in first-occurrence order, resolved onto `nodes`.
    pub edges: Vec<ViewEdge>,
}

impl GroupedView {
    /// The ungrouped view: every node drawn individually, edges one-for-one with
    /// `graph.edges`.
    ///
    /// Offering this from the same type means the painter never needs a
    /// "grouping off" branch — switching grouping off just swaps the view. Edges
    /// whose endpoint is missing from the graph are dropped, exactly as in the
    /// grouped constructor.
    pub fn identity(graph: &Graph) -> Self {
        let index = node_index(graph);
        let nodes = (0..graph.nodes.len()).map(ViewNode::Individual).collect();
        let edges = graph
            .edges
            .iter()
            .filter_map(|edge| {
                let source = ViewNode::Individual(*index.get(&edge.source)?);
                let sink = ViewNode::Individual(*index.get(&edge.sink)?);
                Some(ViewEdge { source, sink })
            })
            .collect();
        Self {
            groups: Vec::new(),
            nodes,
            edges,
        }
    }

    /// Index of the group that owns `node`, if any.
    ///
    /// This asks the groups rather than [`Self::nodes`] on purpose: an expanded
    /// group draws its members individually yet still owns them, and the painter
    /// needs that ownership to place the outline and the label.
    pub fn group_of(&self, node: usize) -> Option<usize> {
        self.groups
            .iter()
            .position(|group| group.members.binary_search(&node).is_ok())
    }
}

/// Build the grouped view of `graph`.
///
/// `positions` are the drawn node centres and `sizes` their world extents, both
/// parallel to `graph.nodes`; short or empty slices are tolerated (a node without
/// a position simply never merges spatially, a node without a size falls back to
/// [`NOMINAL_WIDTH`] × [`NODE_WORLD_H`]) so a caller mid-build can never panic
/// this pass. `expanded` holds the indices of groups the user opened: an expanded
/// group keeps its entry in [`GroupedView::groups`] but its members are drawn
/// individually, which is what lets the outline stay visible while the contents
/// are laid out in place.
///
/// Only [`NodeKind::Circuit`] nodes group; controllers and jacks always map to
/// [`ViewNode::Individual`] because they are physical endpoints of the patch, not
/// interchangeable copies (design Goals: no grouping of controllers or jacks).
pub fn group_nodes<S: BuildHasher>(
    graph: &Graph,
    positions: &[(f32, f32)],
    sizes: &[(f32, f32)],
    settings: &GroupSettings,
    expanded: &HashSet<usize, S>,
) -> GroupedView {
    if !settings.enabled {
        return GroupedView::identity(graph);
    }

    let index = node_index(graph);
    let candidates: Vec<usize> = (0..graph.nodes.len())
        .filter(|&node| graph.nodes[node].kind == NodeKind::Circuit)
        .collect();

    let mut merged = UnionFind::new(graph.nodes.len());
    for (offset, &a) in candidates.iter().enumerate() {
        for &b in &candidates[offset + 1..] {
            if same_type(graph, a, b) && spatially_close(a, b, positions, sizes) {
                merged.union(a, b);
            }
        }
    }

    let adjacency = circuit_adjacency(graph, &index);
    let mut seen = vec![false; graph.nodes.len()];
    for &a in &candidates {
        for b in hops_within(a, &adjacency, &mut seen) {
            if same_type(graph, a, b) {
                merged.union(a, b);
            }
        }
    }

    // Roots are the components' smallest member, so keying by root orders the
    // groups by ascending first member and every member vector stays ascending.
    let mut members_by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &node in &candidates {
        members_by_root
            .entry(merged.find(node))
            .or_default()
            .push(node);
    }

    let mut groups: Vec<Group> = Vec::new();
    for members in members_by_root.into_values() {
        if members.len() < settings.min_size {
            continue;
        }
        let (center, bbox) = footprint(&members, positions);
        groups.push(Group {
            circuit: graph.nodes[members[0]].circuit.clone(),
            members,
            center,
            bbox,
        });
    }

    let mut nodes: Vec<ViewNode> = (0..graph.nodes.len()).map(ViewNode::Individual).collect();
    for (g, group) in groups.iter().enumerate() {
        if expanded.contains(&g) {
            continue;
        }
        for &member in &group.members {
            nodes[member] = ViewNode::Group(g);
        }
    }

    let mut edges: Vec<ViewEdge> = Vec::new();
    let mut distinct: HashSet<ViewEdge> = HashSet::new();
    for edge in &graph.edges {
        let (Some(&source), Some(&sink)) = (index.get(&edge.source), index.get(&edge.sink)) else {
            continue;
        };
        let edge = ViewEdge {
            source: nodes[source],
            sink: nodes[sink],
        };
        // A cable entirely inside one collapsed group has nothing to draw
        // between; cables that merely share endpoints collapse into one arrow,
        // which is where the drawn-edge reduction comes from (design D3).
        if matches!(
            (edge.source, edge.sink),
            (ViewNode::Group(a), ViewNode::Group(b)) if a == b
        ) {
            continue;
        }
        if distinct.insert(edge) {
            edges.push(edge);
        }
    }

    GroupedView {
        groups,
        nodes,
        edges,
    }
}

/// Whether two nodes are interchangeable copies of one circuit type.
fn same_type(graph: &Graph, a: usize, b: usize) -> bool {
    graph.nodes[a].circuit == graph.nodes[b].circuit
}

/// Whether two nodes' drawn extents sit close enough to read as one stack.
///
/// Positions are node centres, so the threshold is the mean extent per axis
/// scaled by [`GROUP_PROXIMITY_RATIO`]: symmetric, and it grows with node width
/// so a wide node does not need to sit further away to stay separate. A node
/// with no position cannot be judged close, so that pair never merges
/// spatially — silently merging on unknown geometry would be a wrong answer,
/// not a lenient one.
fn spatially_close(a: usize, b: usize, positions: &[(f32, f32)], sizes: &[(f32, f32)]) -> bool {
    let (Some(&(ax, ay)), Some(&(bx, by))) = (positions.get(a), positions.get(b)) else {
        return false;
    };
    let ((aw, ah), (bw, bh)) = (size_of(a, sizes), size_of(b, sizes));
    (ax - bx).abs() <= GROUP_PROXIMITY_RATIO * 0.5 * (aw + bw) + PROXIMITY_EPSILON
        && (ay - by).abs() <= GROUP_PROXIMITY_RATIO * 0.5 * (ah + bh) + PROXIMITY_EPSILON
}

/// The drawn extent of node `node`, with the nominal body as the fallback.
fn size_of(node: usize, sizes: &[(f32, f32)]) -> (f32, f32) {
    sizes
        .get(node)
        .copied()
        .unwrap_or((NOMINAL_WIDTH, NODE_WORLD_H))
}

/// Undirected adjacency restricted to circuit→circuit cables.
///
/// Direction is ignored because signal flow must not decide whether two copies
/// of the same circuit sit together; an edge touching a controller or jack is
/// left out because those nodes never group, so a hop through one would join
/// circuits that are not actually neighbours in the grouped view.
fn circuit_adjacency(graph: &Graph, index: &HashMap<&NodeId, usize>) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); graph.nodes.len()];
    for edge in &graph.edges {
        let (Some(&source), Some(&sink)) = (index.get(&edge.source), index.get(&edge.sink)) else {
            continue;
        };
        if graph.nodes[source].kind != NodeKind::Circuit
            || graph.nodes[sink].kind != NodeKind::Circuit
        {
            continue;
        }
        adjacency[source].push(sink);
        adjacency[sink].push(source);
    }
    // Sorted and deduplicated so the traversal — and therefore every merge it
    // triggers — is order-independent.
    for neighbours in &mut adjacency {
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    adjacency
}

/// Circuit nodes at most [`GROUP_HOP_LIMIT`] cables away from `start`, ascending.
///
/// `seen` is a caller-owned scratch buffer: the hop pass runs one traversal per
/// circuit node, so allocating the visited set once and clearing only the
/// entries touched keeps the pass linear in memory.
fn hops_within(start: usize, adjacency: &[Vec<usize>], seen: &mut [bool]) -> Vec<usize> {
    seen[start] = true;
    let mut reached: Vec<usize> = Vec::new();
    let mut frontier = vec![start];
    for _ in 0..GROUP_HOP_LIMIT {
        let mut next = Vec::new();
        for &node in &frontier {
            for &neighbour in &adjacency[node] {
                if !seen[neighbour] {
                    seen[neighbour] = true;
                    reached.push(neighbour);
                    next.push(neighbour);
                }
            }
        }
        frontier = next;
    }
    for &node in &reached {
        seen[node] = false;
    }
    seen[start] = false;
    reached.sort_unstable();
    reached
}

/// Group footprint from its members' drawn positions.
///
/// A member without a position is skipped, so a partially populated `positions`
/// slice still yields a usable box; a group with no known position falls back to
/// the origin rather than inventing a spread.
fn footprint(members: &[usize], positions: &[(f32, f32)]) -> ((f32, f32), (f32, f32, f32, f32)) {
    let mut known = members
        .iter()
        .filter_map(|&member| positions.get(member).copied());
    let Some((x, y)) = known.next() else {
        return ((0.0, 0.0), (0.0, 0.0, 0.0, 0.0));
    };
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (x, y, x, y);
    for (x, y) in known {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    (
        (min_x.midpoint(max_x), min_y.midpoint(max_y)),
        (min_x, min_y, max_x, max_y),
    )
}

/// Node id → node index, first occurrence winning.
///
/// Node ids are unique within a built graph; a hand-built graph that repeats one
/// keeps its first index, so the duplicate is simply not addressable by a cable
/// instead of making edge mapping depend on iteration order.
fn node_index(graph: &Graph) -> HashMap<&NodeId, usize> {
    let mut index = HashMap::with_capacity(graph.nodes.len());
    for (position, node) in graph.nodes.iter().enumerate() {
        index.entry(&node.id).or_insert(position);
    }
    index
}

/// Union-find whose root is always the component's smallest member.
///
/// Choosing the smaller index as the root is what makes the component's root and
/// its first member the same number: the merge outcome is order-independent, but
/// the root identity carries the determinism the caller relies on for ordering.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
        }
    }

    /// Representative of `node`'s component, compressing the walked path so
    /// repeated queries over a large stack stay near-constant.
    fn find(&mut self, node: usize) -> usize {
        let mut root = node;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut cursor = node;
        while self.parent[cursor] != root {
            let next = self.parent[cursor];
            self.parent[cursor] = root;
            cursor = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (root_a, root_b) = (self.find(a), self.find(b));
        if root_a == root_b {
            return;
        }
        let (keep, absorbed) = if root_a < root_b {
            (root_a, root_b)
        } else {
            (root_b, root_a)
        };
        self.parent[absorbed] = keep;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::config::LayoutOrdering;
    use crate::graph::GraphOptions;
    use crate::graph::{Cluster, GraphEdge, GraphNode};
    use crate::latency::CostModel;
    use crate::layout::{estimated_widths, node_world_sizes, solve_columns};
    use crate::patch::{NodeId, Patch};

    /// A `fixtures/` patch's graph, built the way `App::build_graph_state` builds
    /// it (banner groups → clusters, shared default cost model).
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

    /// The two inputs grouping consumes, taken from the real arrangement: the
    /// drawn centres the column solver publishes and the world extents the
    /// renderer draws (ADR 40).
    struct Drawn {
        positions: Vec<(f32, f32)>,
        sizes: Vec<(f32, f32)>,
    }

    fn solved(graph: &Graph) -> Drawn {
        Drawn {
            positions: solve_columns(graph, &estimated_widths(graph), LayoutOrdering::Strict),
            sizes: node_world_sizes(graph),
        }
    }

    /// A circuit node whose id, name and indices all derive from `index`.
    fn circuit_node(circuit: &str, index: usize) -> GraphNode {
        GraphNode {
            id: NodeId::circuit(circuit, index),
            kind: NodeKind::Circuit,
            circuit: circuit.to_string(),
            instance_index: index,
            section_index: index,
        }
    }

    /// A cable between two already-built nodes.
    fn cable(from: &GraphNode, to: &GraphNode, name: &str) -> GraphEdge {
        GraphEdge {
            cable: name.to_string(),
            source: from.id.clone(),
            sink: to.id.clone(),
        }
    }

    fn hand_built(nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> Graph {
        Graph {
            nodes,
            edges,
            ..Graph::default()
        }
    }

    /// Drawn centres no pair can read as one stack: every node thousands of
    /// units from its neighbours on both axes.
    fn far_apart(count: usize) -> Vec<(f32, f32)> {
        (0..count)
            .map(|i| (i as f32 * 4000.0, i as f32 * 3000.0))
            .collect()
    }

    /// Real-sized bodies for `count` nodes.
    fn bodies(count: usize) -> Vec<(f32, f32)> {
        vec![(NOMINAL_WIDTH, NODE_WORLD_H); count]
    }

    /// The longest run of one circuit type drawn contiguously in a single
    /// column, read from the published centres alone.
    ///
    /// The spec scenario names this shape, so the test derives it from the solve
    /// result instead of trusting the implementation under test to report it.
    /// Only circuit nodes are considered: grouping never touches controllers or
    /// jacks, so a run spanning one of those is not the shape under test.
    fn longest_column_run(graph: &Graph, positions: &[(f32, f32)]) -> Vec<usize> {
        let mut columns: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        for (index, node) in graph.nodes.iter().enumerate() {
            let Some(&(x, _)) = positions.get(index) else {
                continue;
            };
            if node.kind == NodeKind::Circuit {
                columns.entry((x * 100.0) as i64).or_default().push(index);
            }
        }

        let mut best: Vec<usize> = Vec::new();
        for mut column in columns.into_values() {
            column.sort_by(|&a, &b| positions[a].1.total_cmp(&positions[b].1));
            let mut start = 0;
            while start < column.len() {
                let circuit = graph.nodes[column[start]].circuit.as_str();
                let mut end = start + 1;
                while end < column.len() && graph.nodes[column[end]].circuit == circuit {
                    end += 1;
                }
                if end - start > best.len() {
                    best = column[start..end].to_vec();
                }
                start = end;
            }
        }
        best
    }

    #[test]
    fn stacked_run_on_scale_fixture_collapses_into_one_group() {
        // The scale fixture draws a long contiguous run of `copy` nodes in one
        // column. Its cables are attributed by name, so copy→copy cables collapse
        // onto the first instance and the run is joined by drawn proximity alone —
        // the spatial half of the union, on real solver output.
        let graph = fixture_graph("own_scale.ini");
        let drawn = solved(&graph);
        let settings = GroupSettings::default();
        let view = group_nodes(
            &graph,
            &drawn.positions,
            &drawn.sizes,
            &settings,
            &HashSet::new(),
        );

        let run = longest_column_run(&graph, &drawn.positions);
        assert!(
            run.len() >= settings.min_size,
            "the fixture must draw a stack of at least {} nodes, found {}",
            settings.min_size,
            run.len()
        );
        let circuit = graph.nodes[run[0]].circuit.as_str();

        let ViewNode::Group(group) = view.nodes[run[0]] else {
            panic!(
                "a contiguous stack of {} `{circuit}` nodes must collapse, got {:?}",
                run.len(),
                view.nodes[run[0]]
            );
        };
        for &member in &run {
            assert_eq!(
                view.nodes[member],
                ViewNode::Group(group),
                "every node of the stack must draw as the one group box"
            );
            assert!(
                view.groups[group].members.contains(&member),
                "node {member} must be owned by the group it draws as"
            );
        }
        assert_eq!(view.groups[group].circuit, circuit);
        assert!(
            view.groups[group]
                .members
                .windows(2)
                .all(|pair| pair[0] < pair[1]),
            "members must be ascending: {:?}",
            view.groups[group].members
        );

        // Spec: the drawn node count falls by the member count minus one.
        let collapsed: usize = view
            .groups
            .iter()
            .map(|group| group.members.len() - 1)
            .sum();
        let stand_ins: HashSet<ViewNode> = view.nodes.iter().copied().collect();
        assert_eq!(view.nodes.len() - stand_ins.len(), collapsed);
    }

    #[test]
    fn two_cable_hops_group_far_apart_same_type_nodes() {
        // `copy` — `mixer` — `copy`: the two copies are two cable hops apart and
        // drawn thousands of units from each other, so only the hop rule can
        // join them.
        let nodes = vec![
            circuit_node("copy", 0),
            circuit_node("mixer", 0),
            circuit_node("copy", 1),
        ];
        let edges = vec![
            cable(&nodes[0], &nodes[1], "_A"),
            cable(&nodes[1], &nodes[2], "_B"),
        ];
        let graph = hand_built(nodes, edges);
        let settings = GroupSettings {
            enabled: true,
            min_size: 2,
        };

        let view = group_nodes(
            &graph,
            &far_apart(graph.nodes.len()),
            &bodies(graph.nodes.len()),
            &settings,
            &HashSet::new(),
        );

        assert_eq!(view.groups.len(), 1, "the two-hop pair must form one group");
        assert_eq!(view.groups[0].circuit, "copy");
        assert_eq!(view.groups[0].members, vec![0, 2]);
        assert_eq!(view.nodes[0], ViewNode::Group(0));
        assert_eq!(view.nodes[2], ViewNode::Group(0));
        assert_eq!(
            view.nodes[1],
            ViewNode::Individual(1),
            "the bridge circuit is not a member"
        );

        // One hop is inside the same budget.
        let pair = vec![circuit_node("copy", 0), circuit_node("copy", 1)];
        let direct = hand_built(pair.clone(), vec![cable(&pair[0], &pair[1], "_C")]);
        let direct_view = group_nodes(
            &direct,
            &far_apart(direct.nodes.len()),
            &bodies(direct.nodes.len()),
            &settings,
            &HashSet::new(),
        );
        assert_eq!(direct_view.groups.len(), 1);
        assert_eq!(direct_view.groups[0].members, vec![0, 1]);
    }

    #[test]
    fn three_cable_hops_leave_same_type_nodes_individual() {
        // `copy` — `mixer` — `vca` — `copy` is three hops: one past the limit.
        let nodes = vec![
            circuit_node("copy", 0),
            circuit_node("mixer", 0),
            circuit_node("vca", 0),
            circuit_node("copy", 1),
        ];
        let edges = vec![
            cable(&nodes[0], &nodes[1], "_A"),
            cable(&nodes[1], &nodes[2], "_B"),
            cable(&nodes[2], &nodes[3], "_C"),
        ];
        let graph = hand_built(nodes, edges);
        let settings = GroupSettings {
            enabled: true,
            min_size: 2,
        };

        let view = group_nodes(
            &graph,
            &far_apart(graph.nodes.len()),
            &bodies(graph.nodes.len()),
            &settings,
            &HashSet::new(),
        );

        assert!(
            view.groups.is_empty(),
            "three hops must not group, got {:?}",
            view.groups
        );
        for (index, drawn) in view.nodes.iter().enumerate() {
            assert_eq!(*drawn, ViewNode::Individual(index));
        }
    }

    #[test]
    fn component_below_the_minimum_stays_individual() {
        // own_two_ctrl.ini draws two `p2b8` nodes in one column and two `copy`
        // nodes — every same-type component has two members, below the default
        // minimum of five.
        let graph = fixture_graph("own_two_ctrl.ini");
        let drawn = solved(&graph);
        let stack_pair: Vec<usize> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Circuit && node.circuit == "p2b8")
            .map(|(index, _)| index)
            .collect();
        assert_eq!(stack_pair.len(), 2, "the fixture must hold the small stack");

        let view = group_nodes(
            &graph,
            &drawn.positions,
            &drawn.sizes,
            &GroupSettings::default(),
            &HashSet::new(),
        );
        assert!(
            view.groups.is_empty(),
            "a two-member component must not become a group, got {:?}",
            view.groups
        );
        for (index, drawn) in view.nodes.iter().enumerate() {
            assert_eq!(*drawn, ViewNode::Individual(index));
        }

        // The member count is what held it back: at a minimum of two the same
        // stack collapses.
        let permissive = GroupSettings {
            enabled: true,
            min_size: 2,
        };
        let grouped = group_nodes(
            &graph,
            &drawn.positions,
            &drawn.sizes,
            &permissive,
            &HashSet::new(),
        );
        assert_eq!(grouped.groups.len(), 1);
        assert_eq!(grouped.groups[0].members, stack_pair);
        assert_eq!(grouped.groups[0].circuit, "p2b8");
    }

    #[test]
    fn identical_inputs_yield_identical_views() {
        let first = fixture_graph("own_scale.ini");
        let second = fixture_graph("own_scale.ini");
        let drawn = solved(&first);
        let rebuilt = solved(&second);
        assert_eq!(drawn.positions, rebuilt.positions);
        let settings = GroupSettings::default();

        let once = group_nodes(
            &first,
            &drawn.positions,
            &drawn.sizes,
            &settings,
            &HashSet::new(),
        );
        let twice = group_nodes(
            &first,
            &drawn.positions,
            &drawn.sizes,
            &settings,
            &HashSet::new(),
        );
        assert_eq!(once.groups, twice.groups);
        assert_eq!(once.nodes, twice.nodes);
        assert_eq!(once.edges, twice.edges);

        assert!(!once.groups.is_empty(), "the fixture must actually group");
        assert!(!once.edges.is_empty());
        assert_eq!(
            once,
            group_nodes(
                &second,
                &rebuilt.positions,
                &rebuilt.sizes,
                &settings,
                &HashSet::new()
            ),
            "a freshly built graph from the same file must group identically"
        );
    }

    #[test]
    fn expanded_group_keeps_its_members_individual() {
        let nodes: Vec<GraphNode> = (0..5).map(|i| circuit_node("encoder", i)).collect();
        let positions: Vec<(f32, f32)> = (0..5).map(|i| (0.0, i as f32 * NODE_WORLD_H)).collect();
        let sizes = bodies(nodes.len());
        let graph = hand_built(nodes, Vec::new());
        let settings = GroupSettings::default();

        let collapsed = group_nodes(&graph, &positions, &sizes, &settings, &HashSet::new());
        assert_eq!(
            collapsed.groups.len(),
            1,
            "five stacked encoders must form one group"
        );
        assert_eq!(collapsed.groups[0].members, vec![0, 1, 2, 3, 4]);

        let expanded: HashSet<usize> = HashSet::from([0]);
        let view = group_nodes(&graph, &positions, &sizes, &settings, &expanded);
        assert_eq!(
            view.groups, collapsed.groups,
            "expanding keeps the group entry"
        );
        for (index, drawn) in view.nodes.iter().enumerate() {
            assert_eq!(
                *drawn,
                ViewNode::Individual(index),
                "an expanded member draws on its own"
            );
        }
        assert_eq!(
            view.group_of(3),
            Some(0),
            "an expanded member is still owned by its group"
        );
    }

    #[test]
    fn disabled_grouping_returns_the_identity_view() {
        let graph = fixture_graph("own_scale.ini");
        let drawn = solved(&graph);
        let off = GroupSettings {
            enabled: false,
            ..GroupSettings::default()
        };

        let view = group_nodes(
            &graph,
            &drawn.positions,
            &drawn.sizes,
            &off,
            &HashSet::new(),
        );
        assert_eq!(view, GroupedView::identity(&graph));
        assert!(view.groups.is_empty());
        for (index, drawn) in view.nodes.iter().enumerate() {
            assert_eq!(*drawn, ViewNode::Individual(index));
        }

        let on = group_nodes(
            &graph,
            &drawn.positions,
            &drawn.sizes,
            &GroupSettings::default(),
            &HashSet::new(),
        );
        assert!(!on.groups.is_empty());
        assert_ne!(on, view, "the toggle must change the drawn view");
    }

    #[test]
    fn cables_deduplicate_and_intra_group_cables_drop() {
        // Five stacked encoders collapse; three of them fan out onto one `vca`,
        // `vca` feeds a `mixer` that feeds back into the group, and one cable is
        // internal to the group.
        let mut nodes: Vec<GraphNode> = (0..5).map(|i| circuit_node("encoder", i)).collect();
        nodes.push(circuit_node("vca", 0));
        nodes.push(circuit_node("mixer", 0));
        let edges = vec![
            cable(&nodes[0], &nodes[5], "_A"),
            cable(&nodes[1], &nodes[5], "_B"),
            cable(&nodes[2], &nodes[5], "_C"),
            cable(&nodes[3], &nodes[4], "_D"),
            cable(&nodes[5], &nodes[6], "_E"),
            cable(&nodes[6], &nodes[0], "_F"),
        ];
        let graph = hand_built(nodes, edges);

        let positions = vec![
            (0.0, 0.0),
            (0.0, NODE_WORLD_H),
            (0.0, 2.0 * NODE_WORLD_H),
            (0.0, 3.0 * NODE_WORLD_H),
            (0.0, 4.0 * NODE_WORLD_H),
            (900.0, 900.0),
            (1800.0, 1800.0),
        ];
        let sizes = bodies(graph.nodes.len());
        let view = group_nodes(
            &graph,
            &positions,
            &sizes,
            &GroupSettings::default(),
            &HashSet::new(),
        );

        assert_eq!(view.groups.len(), 1);
        assert_eq!(view.groups[0].members, vec![0, 1, 2, 3, 4]);
        let group = ViewNode::Group(0);
        assert_eq!(view.nodes[5], ViewNode::Individual(5));
        assert_eq!(view.nodes[6], ViewNode::Individual(6));

        assert_eq!(
            view.edges,
            vec![
                ViewEdge {
                    source: group,
                    sink: ViewNode::Individual(5),
                },
                ViewEdge {
                    source: ViewNode::Individual(5),
                    sink: ViewNode::Individual(6),
                },
                ViewEdge {
                    source: ViewNode::Individual(6),
                    sink: group,
                },
            ],
            "three fan-out cables must collapse into one and the internal cable must vanish"
        );
    }
}
