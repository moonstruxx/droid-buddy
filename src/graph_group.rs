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

/// How narrow one axis must be for a group to read as collinear (design D5b).
///
/// The extent rule asks about *shape*, not *length*: a stack is collinear — its
/// members share one column or one row, however many of them there are — while a
/// type scattered over the canvas is wide on both axes. So a group encloses its
/// members when either axis is within this multiple of the nominal body on that
/// axis: [`NOMINAL_WIDTH`] across, or [`NODE_WORLD_H`] down. A fixed span would
/// decide it by length instead and get both cases backwards — a five-member column
/// already spans four body heights, so a span large enough to hold the stack it was
/// meant to enclose would also swallow genuine spreads.
const GROUP_THIN_RATIO: f32 = 1.5;

/// Which same-type circuits the pass is allowed to put in one group (design D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupPolicy {
    /// Every circuit of one type joins the same group, wherever it is drawn — the
    /// default, because a run is broken by any interleaved circuit type and the
    /// ask is "all the encoders", not "this column of them".
    #[default]
    Type,
    /// Only circuits drawn next to each other or at most [`GROUP_HOP_LIMIT`]
    /// cables apart join, so a group keeps a positional meaning.
    Run,
}

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
    /// How membership is decided; see [`GroupPolicy`].
    pub policy: GroupPolicy,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            min_size: 5,
            policy: GroupPolicy::Type,
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
    /// Centre of the drawn box: where the collapsed box is placed, so grouping
    /// needs no solve of its own (design D1).
    pub center: (f32, f32),
    /// `(min_x, min_y, max_x, max_y)` of the box drawn in a *collapsed* group's
    /// place — the members' union when they are collinear, one nominal body at
    /// their centroid when the type is spread over both axes. The private
    /// `footprint` helper owns the rule (design D5b).
    ///
    /// This is the collapsed box only, not the members' extent. Anything that must
    /// reach a group's members — the expanded outline, an open member's hit area —
    /// reads [`Self::members_bbox`]; reading this one for that purpose is what
    /// shrinks an open group's outline away from the bodies it is supposed to
    /// enclose.
    pub bbox: (f32, f32, f32, f32),
    /// `(min_x, min_y, max_x, max_y)` union of the members' drawn extents, in the
    /// same corner convention as [`Self::bbox`] (positions are top-left corners, so
    /// a caller grows it by one body to reach the drawn frames) and **never**
    /// compacted, however far the type is spread.
    ///
    /// An expanded group is only visible as the outline around its members, drawn
    /// and hit-tested from this union, so a member body still answers as part of
    /// its open group (design D5).
    pub members_bbox: (f32, f32, f32, f32),
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
/// `NOMINAL_WIDTH` × [`NODE_WORLD_H`]) so a caller mid-build can never panic
/// this pass. `expanded` holds the indices of groups the user opened: an expanded
/// group keeps its entry in [`GroupedView::groups`] but its members are drawn
/// individually, which is what lets the outline stay visible while the contents
/// are laid out in place.
///
/// Only [`NodeKind::Circuit`] nodes group; controllers and jacks always map to
/// [`ViewNode::Individual`] because they are physical endpoints of the patch, not
/// interchangeable copies (design Goals: no grouping of controllers or jacks).
///
/// Which circuits join one group is [`GroupSettings::policy`]'s decision: every
/// instance of a type, or only a drawn/cabled run of them. Either way the
/// `min_size` gate and the ascending-index determinism apply unchanged.
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
    match settings.policy {
        // Membership is the type itself: no positional and no cabling test, so a
        // circuit of another type between two members cannot break the group.
        // Candidates ascend, so the first member of each type is the smallest one
        // and the merge order stays deterministic.
        GroupPolicy::Type => {
            let mut first_of_type: HashMap<&str, usize> = HashMap::new();
            for &node in &candidates {
                let circuit = graph.nodes[node].circuit.as_str();
                let first = *first_of_type.entry(circuit).or_insert(node);
                merged.union(first, node);
            }
        }
        // Membership is a place: drawn proximity first, then a short cable chain
        // that the layout did not draw next to each other.
        GroupPolicy::Run => {
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
        let footprint = footprint(&members, positions);
        groups.push(Group {
            circuit: graph.nodes[members[0]].circuit.clone(),
            members,
            center: footprint.center,
            bbox: footprint.drawn,
            members_bbox: footprint.members,
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

/// The extents [`footprint`] derives for one group, kept apart so no caller can
/// mistake the compacted box for the members' own extent.
struct Footprint {
    /// Where the collapsed box is placed (see [`Group::center`]).
    center: (f32, f32),
    /// The collapsed box itself (see [`Group::bbox`]).
    drawn: (f32, f32, f32, f32),
    /// The members' union, never compacted (see [`Group::members_bbox`]).
    members: (f32, f32, f32, f32),
}

/// Group footprint from its members' drawn positions — one extent rule for both
/// policies (design D5b).
///
/// A group says "these are all the same kind of thing", so under
/// [`GroupPolicy::Type`] its members can sit anywhere on the canvas and their raw
/// union would enclose the graph — a box that swallows what it is supposed to
/// summarise. The rule is a test of shape rather than of size: when the union is
/// thin on either axis — within [`GROUP_THIN_RATIO`] of one nominal body across, or
/// of one body height down — the members are collinear, a single column or row of
/// any length, and their union is the honest box. When both axes are wide the type
/// is spread in two dimensions, so the box is one nominal body centred on the
/// members' centroid and the group draws compactly where its type concentrates. A
/// run component is always a tight cluster, so it takes the enclosing branch
/// unchanged.
///
/// Both extents are returned: `drawn` is all a collapsed group paints, while
/// `members` is the union the expanded outline and its hit box cover even when
/// `drawn` compacted to a nominal body.
///
/// A member without a position is skipped, so a partially populated `positions`
/// slice still yields a usable box; a group with no known position falls back to
/// the origin rather than inventing a spread.
fn footprint(members: &[usize], positions: &[(f32, f32)]) -> Footprint {
    let mut known = members
        .iter()
        .filter_map(|&member| positions.get(member).copied());
    let Some((x, y)) = known.next() else {
        return Footprint {
            center: (0.0, 0.0),
            drawn: (0.0, 0.0, 0.0, 0.0),
            members: (0.0, 0.0, 0.0, 0.0),
        };
    };
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (x, y, x, y);
    let (mut sum_x, mut sum_y, mut count) = (x, y, 1_u32);
    for (x, y) in known {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        sum_x += x;
        sum_y += y;
        count += 1;
    }
    let members_bbox = (min_x, min_y, max_x, max_y);
    let collinear = max_x - min_x <= GROUP_THIN_RATIO * NOMINAL_WIDTH
        || max_y - min_y <= GROUP_THIN_RATIO * NODE_WORLD_H;
    if collinear {
        return Footprint {
            center: (min_x.midpoint(max_x), min_y.midpoint(max_y)),
            drawn: members_bbox,
            members: members_bbox,
        };
    }
    let divisor = count as f32;
    let centroid = (sum_x / divisor, sum_y / divisor);
    Footprint {
        center: centroid,
        drawn: (
            centroid.0 - NOMINAL_WIDTH / 2.0,
            centroid.1 - NODE_WORLD_H / 2.0,
            centroid.0 + NOMINAL_WIDTH / 2.0,
            centroid.1 + NODE_WORLD_H / 2.0,
        ),
        members: members_bbox,
    }
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

    use crate::config::LayoutOrdering;
    use crate::graph::{GraphEdge, GraphNode};
    use crate::layout::{estimated_widths, node_world_sizes, solve_columns};
    use crate::patch::NodeId;
    use crate::test_support::fixture_graph;

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
        // The hop rule is run-policy behaviour; the type policy is the default but
        // would group the pair for the wrong reason.
        let settings = GroupSettings {
            enabled: true,
            min_size: 2,
            policy: GroupPolicy::Run,
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
        // Explicit run policy: under the default type policy these two copies are
        // members of one group regardless of cable distance, which is the shape the
        // sibling type-policy test covers.
        let settings = GroupSettings {
            enabled: true,
            min_size: 2,
            policy: GroupPolicy::Run,
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
        // stack collapses. Run policy, because this asserts the minimum-size gate
        // on one drawn stack rather than the default type policy's reach.
        let permissive = GroupSettings {
            enabled: true,
            min_size: 2,
            policy: GroupPolicy::Run,
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

    /// Design D5b: a stack is collinear, so its *length* must not decide whether the
    /// box encloses it. Five members one body apart already span four body heights —
    /// more than any compact cluster needs — and the box still wraps the stack.
    #[test]
    fn a_tall_column_stack_is_enclosed() {
        let nodes: Vec<GraphNode> = (0..5).map(|i| circuit_node("encoder", i)).collect();
        let positions: Vec<(f32, f32)> = (0..5)
            .map(|i| (120.0, 80.0 + i as f32 * NODE_WORLD_H))
            .collect();
        let graph = hand_built(nodes, Vec::new());
        let view = group_nodes(
            &graph,
            &positions,
            &bodies(graph.nodes.len()),
            &GroupSettings::default(),
            &HashSet::new(),
        );

        assert_eq!(view.groups.len(), 1);
        let group = &view.groups[0];
        let (top, bottom) = (positions[0], positions[4]);
        assert!(
            bottom.1 - top.1 > 3.0 * NODE_WORLD_H,
            "the fixture must stack taller than a compact cluster spans"
        );
        assert_eq!(group.members_bbox, (top.0, top.1, bottom.0, bottom.1));
        assert_eq!(
            group.bbox, group.members_bbox,
            "a collinear stack is enclosed, however long"
        );
    }

    /// Design D5b: a type spread over both axes is not a stack, so the drawn box
    /// compacts to one nominal body at the centroid — while `members_bbox` keeps the
    /// members' union, which is what an expanded group's outline covers.
    #[test]
    fn a_two_axis_spread_compacts_the_drawn_box_but_keeps_the_member_union() {
        let nodes: Vec<GraphNode> = (0..5).map(|i| circuit_node("encoder", i)).collect();
        let positions = vec![
            (0.0, 0.0),
            (400.0, 0.0),
            (0.0, 400.0),
            (400.0, 400.0),
            (800.0, 200.0),
        ];
        let graph = hand_built(nodes, Vec::new());
        let view = group_nodes(
            &graph,
            &positions,
            &bodies(graph.nodes.len()),
            &GroupSettings::default(),
            &HashSet::new(),
        );

        assert_eq!(view.groups.len(), 1);
        let group = &view.groups[0];
        assert_eq!(group.members_bbox, (0.0, 0.0, 800.0, 400.0));
        assert_eq!(group.center, (320.0, 200.0), "the members' centroid");
        assert_eq!(
            group.bbox,
            (
                group.center.0 - NOMINAL_WIDTH / 2.0,
                group.center.1 - NODE_WORLD_H / 2.0,
                group.center.0 + NOMINAL_WIDTH / 2.0,
                group.center.1 + NODE_WORLD_H / 2.0,
            ),
            "a two-dimensional spread draws one nominal body at the centroid"
        );
        assert_ne!(
            group.bbox, group.members_bbox,
            "the compacted box is not the member union"
        );
    }

    /// Design D5b's other thin axis: a single row is collinear too, so its box
    /// encloses it exactly as a column's does — the rule tests shape, not length.
    #[test]
    fn a_wide_row_is_enclosed_like_a_column() {
        let nodes: Vec<GraphNode> = (0..5).map(|i| circuit_node("encoder", i)).collect();
        let positions: Vec<(f32, f32)> =
            (0..5).map(|i| (100.0 + i as f32 * 400.0, 120.0)).collect();
        let graph = hand_built(nodes, Vec::new());
        let view = group_nodes(
            &graph,
            &positions,
            &bodies(graph.nodes.len()),
            &GroupSettings::default(),
            &HashSet::new(),
        );

        assert_eq!(view.groups.len(), 1);
        let group = &view.groups[0];
        assert!(
            positions[4].0 - positions[0].0 > GROUP_THIN_RATIO * NOMINAL_WIDTH,
            "the row must span more than the thin-axis bound, or it proves nothing"
        );
        assert_eq!(group.members_bbox, (100.0, 120.0, 1700.0, 120.0));
        assert_eq!(
            group.bbox, group.members_bbox,
            "a collinear row is enclosed, however wide"
        );
        assert_eq!(group.center, (900.0, 120.0));
    }

    /// Spec scenario "A type collapses into one box": at the default type policy
    /// the whole population of one type is one group, wherever the solver drew it.
    #[test]
    fn type_policy_collapses_a_whole_type_on_a_real_fixture() {
        // influence_outlier.ini carries twenty-five `copy` nodes plus one `p2b8`
        // and one `button` — a repeated type at a real patch's order of magnitude,
        // so the group is not an artefact of a hand-built stack.
        let graph = fixture_graph("influence_outlier.ini");
        let drawn = solved(&graph);
        let settings = GroupSettings::default();
        assert_eq!(
            settings.policy,
            GroupPolicy::Type,
            "the default policy is the one under test"
        );

        let copies: Vec<usize> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Circuit && node.circuit == "copy")
            .map(|(index, _)| index)
            .collect();
        assert_eq!(copies.len(), 25, "the fixture must draw the repeated type");
        assert!(
            copies.windows(2).all(|pair| pair[0] < pair[1]),
            "indices must ascend: {copies:?}"
        );

        let view = group_nodes(
            &graph,
            &drawn.positions,
            &drawn.sizes,
            &settings,
            &HashSet::new(),
        );

        let ViewNode::Group(group) = view.nodes[copies[0]] else {
            panic!(
                "all 25 `copy` circuits must collapse, got {:?}",
                view.nodes[copies[0]]
            );
        };
        assert_eq!(
            view.groups[group].members, copies,
            "the group holds exactly that type's nodes, ascending"
        );
        assert_eq!(view.groups[group].circuit, "copy");
        assert_eq!(view.groups.len(), 1, "no other type reaches the minimum");
        for &member in &copies {
            assert_eq!(
                view.nodes[member],
                ViewNode::Group(group),
                "node {member} must draw as the type's one group box"
            );
        }

        // One stand-in replaces the whole population, and the singleton types keep
        // their own body each, so the drawn count falls by the population minus one.
        let stand_ins: HashSet<ViewNode> = copies.iter().map(|&node| view.nodes[node]).collect();
        assert_eq!(stand_ins.len(), 1);
        let singles: Vec<usize> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Circuit && node.circuit != "copy")
            .map(|(index, _)| index)
            .collect();
        assert_eq!(singles.len(), 2, "the fixture's two other circuit types");
        for &node in &singles {
            assert_eq!(view.nodes[node], ViewNode::Individual(node));
        }
        // The controllers the fixture's register cables introduce never group, so
        // the whole reduction is the one copy group's population minus one.
        let collapsed: usize = view
            .groups
            .iter()
            .map(|group| group.members.len() - 1)
            .sum();
        assert_eq!(collapsed, copies.len() - 1);
        let drawn_bodies = view
            .nodes
            .iter()
            .copied()
            .collect::<HashSet<ViewNode>>()
            .len();
        assert_eq!(
            view.nodes.len() - drawn_bodies,
            collapsed,
            "one stand-in per group replaces its members"
        );
    }

    /// The run policy is strictly stricter than the type policy on one graph: the
    /// same six copies that `Type` merges into one box stay apart under `Run`,
    /// because the only drawn run is three members short of the minimum.
    #[test]
    fn run_policy_is_stricter_than_type_policy_on_the_same_graph() {
        let nodes: Vec<GraphNode> = (0..6).map(|i| circuit_node("copy", i)).collect();
        // Three copies stacked in one column, three far from everything — and no
        // cables at all, so only the spatial half of the run union can act.
        let positions = vec![
            (0.0, 0.0),
            (0.0, NODE_WORLD_H),
            (0.0, 2.0 * NODE_WORLD_H),
            (4000.0, 4000.0),
            (8000.0, 8000.0),
            (12000.0, 12000.0),
        ];
        let sizes = bodies(nodes.len());
        let graph = hand_built(nodes, Vec::new());

        let typed = group_nodes(
            &graph,
            &positions,
            &sizes,
            &GroupSettings::default(),
            &HashSet::new(),
        );
        let run = group_nodes(
            &graph,
            &positions,
            &sizes,
            &GroupSettings {
                enabled: true,
                min_size: 5,
                policy: GroupPolicy::Run,
            },
            &HashSet::new(),
        );

        assert_eq!(typed.groups.len(), 1);
        assert_eq!(typed.groups[0].members, (0..6).collect::<Vec<_>>());
        assert!(
            run.groups.is_empty(),
            "the only drawn run has three members, below the minimum: {:?}",
            run.groups
        );

        let typed_bodies: HashSet<ViewNode> = typed.nodes.iter().copied().collect();
        let run_bodies: HashSet<ViewNode> = run.nodes.iter().copied().collect();
        assert_eq!(
            typed_bodies.len(),
            1,
            "one box stands in for all six copies under the type policy"
        );
        assert_eq!(
            run_bodies.len(),
            6,
            "every copy draws individually under the run policy"
        );
        assert!(
            run_bodies.len() > typed_bodies.len(),
            "the run policy draws more bodies than the type policy"
        );
    }

    /// The policy difference in isolation: two same-type circuits drawn far apart
    /// and uncabled are one group under `Type` and two individual bodies under
    /// `Run`, at the same minimum size.
    #[test]
    fn far_apart_same_type_pair_groups_only_under_type_policy() {
        let nodes = vec![circuit_node("copy", 0), circuit_node("copy", 1)];
        let positions = vec![(0.0, 0.0), (6000.0, 6000.0)];
        let sizes = bodies(nodes.len());
        let graph = hand_built(nodes, Vec::new());
        let permissive = |policy| GroupSettings {
            enabled: true,
            min_size: 2,
            policy,
        };

        let typed = group_nodes(
            &graph,
            &positions,
            &sizes,
            &permissive(GroupPolicy::Type),
            &HashSet::new(),
        );
        assert_eq!(typed.groups.len(), 1);
        assert_eq!(typed.groups[0].members, vec![0, 1]);
        assert_eq!(typed.nodes[0], ViewNode::Group(0));
        assert_eq!(typed.nodes[1], ViewNode::Group(0));

        let run = group_nodes(
            &graph,
            &positions,
            &sizes,
            &permissive(GroupPolicy::Run),
            &HashSet::new(),
        );
        assert!(
            run.groups.is_empty(),
            "no cable and no proximity is not a run: {:?}",
            run.groups
        );
        for (index, drawn) in run.nodes.iter().enumerate() {
            assert_eq!(*drawn, ViewNode::Individual(index));
        }
    }

    /// The minimum-size gate is the same under both policies, and it cuts both
    /// ways: a five-member stack groups while a four-member stack of another type
    /// stays individual, so neither policy is deciding by reach alone.
    #[test]
    fn minimum_size_gate_holds_under_both_policies() {
        // Five `copy` stacked in one column (a group at the default minimum) and
        // four `vca` stacked in another (one below it).
        let mut nodes: Vec<GraphNode> = (0..5).map(|i| circuit_node("copy", i)).collect();
        nodes.extend((0..4).map(|i| circuit_node("vca", i)));
        let mut positions: Vec<(f32, f32)> =
            (0..5).map(|i| (0.0, i as f32 * NODE_WORLD_H)).collect();
        positions.extend((0..4).map(|i| (900.0, i as f32 * NODE_WORLD_H)));
        let sizes = bodies(nodes.len());
        let graph = hand_built(nodes, Vec::new());

        for policy in [GroupPolicy::Type, GroupPolicy::Run] {
            let view = group_nodes(
                &graph,
                &positions,
                &sizes,
                &GroupSettings {
                    enabled: true,
                    min_size: 5,
                    policy,
                },
                &HashSet::new(),
            );
            assert_eq!(
                view.groups.len(),
                1,
                "{policy:?}: only the copy stack reaches the minimum"
            );
            assert_eq!(view.groups[0].members, (0..5).collect::<Vec<_>>());
            assert_eq!(view.groups[0].circuit, "copy");
            for (node, drawn) in view.nodes.iter().enumerate().skip(5) {
                assert_eq!(
                    *drawn,
                    ViewNode::Individual(node),
                    "{policy:?}: a four-member component stays individual"
                );
            }
        }
    }
}
