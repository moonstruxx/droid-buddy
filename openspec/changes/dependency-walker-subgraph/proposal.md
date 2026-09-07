## Why

Large patches are hard to read as one graph. DroidGraph cuts them down with a lazy upstream walk: starting from one node, it follows incoming edges to everything the node depends on, and stops at controllers, which are the sources where humans act. The graph here has a downstream `influence_subtree` (what a token affects) and a FILTERED pane, but no way to answer the reverse question for a physical output: what feeds this output, transitively? With change B's controller and jack nodes in place, the upstream walk has proper leaves (controllers, input jacks) and a natural root (an output jack or any node).

## What Changes

- `Graph` gains an upstream dependency walk: starting from a root node, follow reversed edges (incoming) breadth-first, cycle-safe, stopping at controller and input-jack nodes, which are sources and have no upstream dependencies.
- On the graph pane, `f` filters the view to the upstream dependency subgraph of the hovered node (falling back to the shared circuit selection): only the reachable nodes and the edges among them render, solved independently and deterministically. The status bar reports `Dependencies of <node>: N nodes`. `f` again or Esc restores the full graph.
- The filter is presentation-only: it never mutates the graph model, the influence traversal, or the patch.

## Capabilities

### New Capabilities

- signal-flow-graph: a new requirement "Upstream dependency subgraph" covering the walk and the filtered presentation.

### Modified Capabilities

- (none)

## Impact

- src/graph.rs: `upstream_dependencies(root: NodeId) -> Vec<NodeId>` (reversed-edge BFS) plus a subgraph-view helper (node + edge subset).
- src/app.rs: dependency-filter state (`dependency_root: Option<NodeId>`) reset on patch load and cleared on graph close.
- src/handler.rs: the `f` key on the graph pane.
- src/ui.rs: rendering the filtered node/edge subset with the existing graph-slot machinery; status integration.
- src/layout.rs: no change; the subset solves through the existing path.

## Non-goals

- No change to the FULL/FILTERED quad view or the downstream influence traversal.
- No incremental/lazy node loading during drag or camera moves; the subgraph is computed once per root.
- No dangling-cable nodes (change F) or select-state interaction (change C) in this change.