# Spec: Optimizer Click → Cross-View Focus

## Capability

**ID:** `optimizer-click-focus`
**Title:** Click optimizer candidate → preview + cross-view focus with before/after diff
**Status:** Proposed

## Requirements

### REQ-1: Click previews and focuses
When the optimizer pane is focused and the user clicks a candidate row (or presses `Enter` on it):
- The candidate's section order is applied to `Patch.sections` (existing preview behavior)
- The graph is rebuilt with the new order (existing `rebuild_graph`)
- A cross-view focus is established on the changed nodes (new)

### REQ-2: Before/after graph diff
The graph pane renders a single overlaid scene showing both states:
- Nodes present in file order but not candidate order → **red** (`graph_node_diff_before`)
- Nodes present in candidate order but not file order → **blue** (`graph_node_diff_after`)
- Nodes present in both → **purple** (`graph_node_diff_both`)
- Nodes unchanged → **dim** (`graph_node_dim`)
- Cables whose latency delta ≠ 0 → **dashed yellow** (`graph_edge_diff_changed`)
- Cables unchanged → normal kind color

### REQ-3: Source viewer jump
- `App.selected_circuit` is set to the first circuit (by file order) whose section index changed
- Source viewer scrolls to that circuit's first occurrence (existing `jump_to_occurrence`)

### REQ-4: Module UI highlight
- `select_component` is called with the corresponding hardware token(s) for the changed circuit
- Module UI highlights the element via existing `selected_component` machinery

### REQ-5: Restore on Esc
- `Esc` while optimizer focused restores original section order (existing)
- Clears cross-view focus and diff state
- Graph reverts to single-state render

### REQ-6: Click focus does not close optimizer
- Optimizer pane remains open after click/Enter
- Clicking another candidate updates the diff/focus to the new candidate

## Data Model

### App additions
```rust
struct OptimizerDiffState {
    before_spec: Option<SceneSpec>,   // cached before preview
    after_spec: Option<SceneSpec>,    // current preview
    diff_nodes: HashMap<NodeId, DiffNodeState>, // Before | After | Both | None
    diff_edges: HashMap<usize, DiffEdgeState>,  // Changed | Unchanged
    focused_circuit: Option<NodeId>,  // first changed circuit
}

enum DiffNodeState { Before, After, Both, None }
enum DiffEdgeState { Changed, Unchanged }
```

### Theme tokens (add to all palettes)
- `graph_node_diff_before` — red
- `graph_node_diff_after` — blue
- `graph_node_diff_both` — purple (mix)
- `graph_edge_diff_changed` — yellow (dashed)
- `graph_node_dim` — existing

## Behavior Details

### Diff Computation
On `optimizer_preview`:
1. Capture `before_spec` from current `SceneSpec` (before reorder)
2. Apply candidate order, `rebuild_graph`
3. Capture `after_spec` from new `SceneSpec`
4. Compute `diff_nodes` by comparing `before_spec.nodes` vs `after_spec.nodes` by `NodeId`
5. Compute `diff_edges` by comparing latency values per cable
6. Find first changed circuit → `focused_circuit`

### Render Path
`gui/graph.rs::paint_scene_in`:
- If `optimizer_diff_state.is_some()` → dual render:
  - Draw before nodes in red, after nodes in blue, both in purple, none dim
  - Draw edges: dashed yellow if `Changed`, else kind color
- Else → single render (existing)

### Focus Wiring
- `focused_circuit` → `App.selected_circuit` (triggers source jump + module UI highlight)
- `optimizer_diff_state` lives on `App`; cleared on `optimizer_close` / `Esc`

## Acceptance Criteria

1. `g o` → click row 1 → graph shows red/blue/purple nodes, yellow dashed changed cables
2. Source viewer auto-scrolls to first changed circuit's occurrence
3. Module UI highlights corresponding hardware elements
4. Click row 2 → diff updates to row 2's delta
5. `Esc` → original order restored, diff cleared, single-state render
6. All existing optimizer tests pass
7. No new config keys