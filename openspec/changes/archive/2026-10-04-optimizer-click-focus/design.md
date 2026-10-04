# Design: Optimizer Click → Cross-View Focus

## Overview

This design adds cross-view focus when clicking an optimizer candidate row. The optimizer pane stays a small-class pane; click/Enter applies the preview and establishes a diff focus across the graph, source viewer, and module UI.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        OPTIMIZER PANE                           │
│  [Row 0: banner (default)  obj 0.12 · avg 1.23→0.89 · max ...] │
│  [Row 1: global min-sum       obj 0.15 · avg 1.45→0.92 · max ...] ▶ click
│  [Row 2: annealing            obj 0.18 · avg 1.67→1.01 · max ...] │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      APP STATE MUTATIONS                        │
│  1. optimizer_preview(idx)                                    │
│     - restore_optimizer_order()                                │
│     - capture before_spec = current SceneSpec                  │
│     - apply_section_order(candidate.order)                     │
│     - rebuild_graph()                                          │
│     - capture after_spec = new SceneSpec                       │
│     - compute diff_nodes / diff_edges                          │
│     - focused_circuit = first_changed_circuit()                │
│     - selected_circuit = focused_circuit                       │
│     - select_component(hw_tokens_for(focused_circuit))         │
│     - optimizer_diff_state = Some(DiffState{...})              │
│     - refit_graph_camera_after_optimizer_rebuild()             │
└─────────────────────────────────────────────────────────────────┘
              │                         │                       │
              ▼                         ▼                       ▼
┌──────────────────┐          ┌──────────────────┐    ┌──────────────────┐
│   GRAPH PANE     │          │  SOURCE VIEWER   │    │   MODULE UI      │
│  (big class)     │          │  (small class)   │    │  (big class)     │
│                  │          │                  │    │                  │
│ Dual render:     │          │ jump_to_occur-   │    │ select_component │
│ - before=red     │          │ rence(focused_   │    │ highlights HW    │
│ - after=blue     │          │  _circuit)       │    │  elements        │
│ - both=purple    │          │                  │    │                  │
│ - none=dim       │          │                  │    │                  │
│ - edges: dashe-  │          │                  │    │                  │
│   d yellow if    │          │                  │    │                  │
│   latency_delta  │          │                  │    │                  │
└──────────────────┘          └──────────────────┘    └──────────────────┘
```

## Data Structures

### App Extensions (src/app.rs)

```rust
/// Optimizer diff state for cross-view focus on preview
struct OptimizerDiffState {
    /// SceneSpec before reorder (captured before apply_section_order)
    before_spec: Option<SceneSpec>,
    /// SceneSpec after reorder (current preview)
    after_spec: Option<SceneSpec>,
    /// Per-node diff state by NodeId
    diff_nodes: HashMap<NodeId, DiffNodeState>,
    /// Per-edge diff state by edge index
    diff_edges: HashMap<usize, DiffEdgeState>,
    /// First circuit whose section index changed (triggers focus)
    focused_circuit: Option<NodeId>,
}

enum DiffNodeState {
    Before,  // in before_spec only
    After,   // in after_spec only
    Both,    // in both (position may differ)
    None,    // in neither (shouldn't happen)
}

enum DiffEdgeState {
    Changed,    // latency delta != 0
    Unchanged,  // latency delta == 0
}
```

### Theme Tokens (src/theme.rs)

Add to all three palettes (`classic`, `terminal`, `mono`):
- `graph_node_diff_before` = `Color::Rgb(220, 50, 50)` (red)
- `graph_node_diff_after` = `Color::Rgb(50, 150, 220)` (blue)
- `graph_node_diff_both` = `Color::Rgb(180, 60, 200)` (purple)
- `graph_edge_diff_changed` = `Color::Rgb(220, 200, 40)` (yellow)
- `graph_node_dim` = existing

## Render Path (src/gui/graph.rs)

### paint_scene_in Diff Branch

```rust
fn paint_scene_in(dst: &Painter, rect: Rect, app: &mut App, theme: &Theme, ...) {
    if let Some(diff) = &app.optimizer_diff_state {
        paint_diff_scene(dst, rect, app, theme, diff);
    } else {
        paint_single_scene(dst, rect, app, theme);  // existing
    }
}
```

### paint_diff_scene

```rust
fn paint_diff_scene(dst: &Painter, rect: Rect, app: &mut App, theme: &Theme, diff: &OptimizerDiffState) {
    // Build node color map
    let node_color: HashMap<NodeId, Color> = diff.diff_nodes.iter().map(|(id, state)| {
        let color = match state {
            DiffNodeState::Before => theme.graph_node_diff_before,
            DiffNodeState::After => theme.graph_node_diff_after,
            DiffNodeState::Both => theme.graph_node_diff_both,
            DiffNodeState::None => theme.graph_node_dim,
        };
        (*id, color)
    }).collect();

    // Draw nodes
    for (i, node) in app.graph.nodes.iter().enumerate() {
        if let Some(&color) = node_color.get(&node.id) {
            let pos = app.graph_positions[i];
            draw_node(dst, pos, node, color, ...);
        }
    }

    // Draw edges with conditional dash
    for (i, edge) in app.graph.edges.iter().enumerate() {
        let dash = diff.diff_edges.get(&i) == Some(&DiffEdgeState::Changed);
        let color = if dash { theme.graph_edge_diff_changed } else { edge_color(edge, theme) };
        draw_edge(dst, app, i, edge, color, dash, ...);
    }
}
```

## Diff Computation (src/app.rs)

```rust
impl App {
    fn compute_optimizer_diff(&mut self, before_spec: &SceneSpec, after_spec: &SceneSpec) {
        let mut diff_nodes = HashMap::new();
        let mut diff_edges = HashMap::new();

        // Nodes: compare by NodeId
        let before_nodes: HashSet<_> = before_spec.nodes.iter().map(|n| n.id.clone()).collect();
        let after_nodes: HashSet<_> = after_spec.nodes.iter().map(|n| n.id.clone()).collect();

        for id in before_nodes.union(&after_nodes) {
            let in_before = before_nodes.contains(id);
            let in_after = after_nodes.contains(id);
            let state = match (in_before, in_after) {
                (true, false) => DiffNodeState::Before,
                (false, true) => DiffNodeState::After,
                (true, true) => DiffNodeState::Both,
                _ => DiffNodeState::None,
            };
            diff_nodes.insert(id.clone(), state);
        }

        // Edges: compare latency per cable (by cable name)
        // before_spec.edges / after_spec.edges carry latency
        for (i, before_edge) in before_spec.edges.iter().enumerate() {
            if let Some(after_edge) = after_spec.edges.iter().find(|e| e.cable == before_edge.cable) {
                let delta = after_edge.latency - before_edge.latency;
                diff_edges.insert(i, if delta.abs() > 1e-6 { DiffEdgeState::Changed } else { DiffEdgeState::Unchanged });
            }
        }

        // First changed circuit (by file order)
        let focused = self.graph.as_ref().and_then(|g| {
            g.nodes.iter().enumerate().find(|(i, n)| {
                matches!(diff_nodes.get(&n.id), Some(DiffNodeState::Before) | Some(DiffNodeState::After) | Some(DiffNodeState::Both))
            }).map(|(i, _)| g.nodes[i].id.clone())
        });

        self.optimizer_diff_state = Some(OptimizerDiffState {
            before_spec: Some(before_spec.clone()),
            after_spec: Some(after_spec.clone()),
            diff_nodes,
            diff_edges,
            focused_circuit: focused,
        });
    }
}
```

## Click Handling (src/gui/overlays.rs + src/handler.rs)

### paint_optimizer

Add row click detection: if pointer over row rect and primary pressed → `optimizer_row_click(idx)` message.

### handler.rs

Route click to `App::optimizer_preview(idx)` (same as Enter) — no new path needed.

## Edge Cases

- **No graph open**: `before_spec`/`after_spec` are `None`; diff render no-ops gracefully
- **Candidate identical to file order**: diff empty → single-state render, no focus jump
- **Multiple changed circuits**: first by file order wins (deterministic)
- **Force vs Column layout**: diff works for both; positions from `SceneSpec`
- **Esc during preview**: `optimizer_close` restores order, clears `optimizer_diff_state`
- **Click another row**: new `before_spec` = current `after_spec`, new preview applied, diff recomputed

## Testing

- Unit: diff computation with synthetic before/after SceneSpecs
- Integration: `g o` → click row → verify diff colors, source jump, module UI highlight
- Regression: existing optimizer tests unchanged