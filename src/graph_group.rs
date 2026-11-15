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
    (ax - bx).abs() <= GROUP_PROXIMITY_RATIO * 0.5 * (aw + bw)
        && (ay - by).abs() <= GROUP_PROXIMITY_RATIO * 0.5 * (ah + bh)
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
