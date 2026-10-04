# Spec: Optimizer Click → Cross-View Focus

## Capability

**ID:** `optimizer-click-focus`
**Title:** Click optimizer candidate → preview + cross-view focus with latency-delta diff
**Status:** Proposed

> As-built note (2026-10-04): implemented in latency-delta form per WIP
> commits 0c8e232/cce1d4f, superseding the earlier dual-SceneSpec proposal
> (no `before_spec`/`after_spec` snapshots, no `compute_optimizer_diff`,
> no `paint_diff_scene`, no purple Both, no dashed yellow). This spec
> describes the as-built behavior.

## Requirements

### REQ-1: Click previews and focuses
When the optimizer pane is focused and the user clicks a candidate row (or presses `Enter` on it):
- The candidate's section order is applied to `Patch.sections` (existing preview behavior)
- The graph is rebuilt with the new order (existing `rebuild_graph`)
- A cross-view focus is established on the first affected circuit (new)

### REQ-2: Latency-delta graph recolor
The graph pane keeps its single render path; edges and node borders recolor against the
file-order baseline captured when the optimizer opened:
- Edge whose forward-loop latency got worse vs baseline → **red** (`graph_edge_diff_before`), solid
- Edge whose latency got better vs baseline → **blue** (`graph_edge_diff_after`), solid
- Edge unchanged → normal kind/latency color
- Node border aggregates its incident edges' direction (worse beats better); unchanged nodes keep their resolved border
- Precedence preserved: topology-error red > dim (disabled / not-selected / uninfluenced) > influence highlight > optimizer diff > latency ramp > register > cable kind

### REQ-3: Source viewer jump
- `App.selected_circuit` is set to the sink of the first edge (file order) whose latency moved beyond epsilon (`first_affected_node`)
- Source viewer scrolls to that circuit's first occurrence (existing `jump_to_occurrence`)
- No changed edge → no focus jump (preview still applies)

### REQ-4: Module UI highlight
- The focused circuit flows through the existing `selected_circuit` machinery, so the module UI highlights the corresponding hardware elements alongside the graph and source jump

### REQ-5: Restore on Esc / r
- `r` (optimizer pane) restores original section order, rebuilds the graph when a preview was active, and clears the latency baseline (`optimizer_diff_state = None`)
- `Esc` closes the optimizer pane via `close_focused_view`, clearing preview + baseline (`drop_optimizer_state`)
- Patch load clears the baseline

### REQ-6: Click focus does not steal optimizer focus
- `select_circuit` opens + focuses the source viewer, so preview hands focus back to the optimizer pane (`pane_holding(ViewType::Optimizer)` → `set_focus`)
- The `j/k/r/Enter` flow keeps working after a preview; status reports `Preview: <label>`
- Optimizer pane remains open after click/Enter; clicking another candidate updates the preview/focus to the new candidate

## Data Model

### App additions (src/app.rs)
```rust
/// File-order per-edge latency baseline captured when the optimizer opens.
/// A section *reorder* keeps the same node set, so a per-node before/after
/// membership diff is a no-op — the meaningful signal is the *latency*
/// change the reorder produces. Parallel to the file-order edge vector
/// (edges sort deterministically by cable/source/sink, so a pure reorder
/// keeps the same order); the cable-name guard makes the diff robust if
/// that invariant ever breaks.
struct OptimizerDiffState {
    /// Cable name per before-order edge index.
    before_cable: Vec<String>,
    /// Forward-loop latency per before-order edge index.
    before_latency: Vec<f32>,
}

/// Per-edge preview direction vs the baseline.
enum OptDir { Worse, Better }
```

### Theme tokens (all palettes)
- `graph_edge_diff_before` — red (worse)
- `graph_edge_diff_after` — blue (better)
- `graph_node_diff_before` — red (worse node border)
- `graph_node_diff_after` — blue (better node border)

## Behavior Details

### Baseline capture
On optimizer open (`open_optimizer`): snapshot the current graph's per-edge
forward-loop latency (`capture_optimizer_baseline`) into `optimizer_diff_state`.

### Preview
On `optimizer_preview(idx)` (click row ≡ `Enter` via `optimizer_row_at` hit-test):
1. Restore file order, apply candidate order, `rebuild_graph`
2. Refit the camera to the re-solved layout (`fit_graph_camera` against the published pane size)
3. `first_affected_node()`: scan current edges in file order against the baseline by cable name; return the sink of the first edge whose latency moved beyond epsilon
4. `select_circuit(node)` for cross-view focus, then hand focus back to the optimizer pane
5. Status `Preview: <label>`

### Render Path
`gui/graph.rs` scene build: per edge, compare current latency to the baseline
(cable-name-guarded); worse → `graph_edge_diff_before`, better →
`graph_edge_diff_after`, under the existing error/influence/disabled
precedence. Per node, aggregate incident direction into the border
(worse beats better; 3px zoom-scaled border).

## Acceptance Criteria

1. `g o` → click row / `Enter` → graph shows red (worse) / blue (better) edges + node borders
2. Source viewer auto-scrolls to first latency-changed circuit's occurrence; module UI highlights it; focus stays on the optimizer pane (`r` still restores)
3. Click another row → preview + recolor + focus update to the new candidate
4. `r` / `Esc` → original order restored, baseline cleared, single-state render
5. All existing optimizer tests pass
6. No new config keys
