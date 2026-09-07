## Context

`App.influence_subtree` walks downstream from a hardware token's root variables over the cable index (structural forward BFS, cycle-safe), answering "what does this token affect". There is no upstream counterpart. The graph has a FILTERED pane in the quad view (`\`), but its filter is not dependency-based. Change B adds controller, input-jack, and output-jack nodes and register edges, which gives the upstream walk its leaves and its most useful root: an output jack. DroidGraph's `DependencyWalker` walks incoming edges on demand from a root, yields each reachable node once, and stops at controllers because their incoming edges (LED writes) are not dependencies: controllers are where humans act, not things a signal depends on.

## Goals / Non-Goals

**Goals:**

- Compute the upstream dependency set of a root node: everything that feeds it, transitively, stopping at controller and input-jack sources.
- Present it as a filtered graph view from the graph pane, with a status summary.

**Non-Goals:**

- Changing the downstream influence traversal or the FULL/FILTERED quad view.
- Lazy/incremental walking during interaction; the set is computed once per root.
- Persisting the filter across patch loads or graph close.

## Decisions

- **The walk is a reversed-edge BFS over the built graph.** `Graph` already holds `edges: Vec<GraphEdge>` with `source`/`sink` ids; the reverse adjacency is `sink -> sources`, built on demand per call. The BFS visits each node once (visited set), so cycles terminate naturally. This is simpler than DroidGraph's lazy `yield` walk and equivalent for a finite graph we already hold in memory.

- **Controller and input-jack nodes are leaves.** A controller's incoming edges are LED writes and button feedback, not things the controller depends on; DroidGraph stops there and so does this walk. Input jacks have no incoming edges by construction (change B classifies them as sources). Output jacks are the typical root: everything feeding `O3` is its dependency set. A controller or input-jack node as root yields just itself, which is the correct degenerate case.

- **The filtered presentation reuses the existing graph-slot machinery.** The dependency set is a node subset; the edges rendered are exactly the graph edges with both endpoints in the set. The subset solves through the existing deterministic solve path (positions parallel to the subset, not the full graph), so the filtered layout is reproducible per root, same as the FILTERED pane contract. The full graph model is untouched; only the rendered subset changes.

- **`f` on the graph pane toggles the filter, rooted at the hovered node.** The hover machinery already exists (`hovered_graph_node`). When nothing is hovered, the shared circuit selection (`selected_circuit`) is the fallback root; when neither exists, `f` is a silent no-op with a status hint, matching the `x`/`p` conventions. Pressing `f` again clears the filter; Esc, patch load, and graph close also clear it. The root is frozen when the filter engages, so moving the mouse does not re-root the walk.

- **The walk lives on `Graph`, the filter state on `App`.** `Graph::upstream_dependencies(root)` is a pure query over the model, unit-testable without App. `App.dependency_root: Option<NodeId>` is presentation state, reset in the same places `graph_camera` is reset.

## Risks / Trade-offs

- [A huge patch yields a dependency set close to the full graph] → the subset still removes the downstream half and unrelated branches; if a root's upstream is everything, the view is the full graph and the status line says so.

- [Reversed-edge BFS per call is O(edges)] → the graph is at DROID scale (tens of circuits); the full graph solve already runs per frame, and this runs once per root toggle.

- [Hover-rooted filter feels unstable] → the root is frozen at toggle time; the status line names the root, and re-toggling re-roots from the current hover.