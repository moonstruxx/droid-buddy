# Proposal: Optimizer Click → Cross-View Focus

## Summary

Clicking a candidate row in the latency optimizer pane (`g o`) will preview the section reorder (as `Enter` does today) **and** focus the affected nodes across all views: the signal-flow graph shows a before/after diff (red = before, blue = after, changed cables dashed yellow), the source viewer jumps to the first changed circuit's occurrence, and the module UI highlights the corresponding hardware elements.

## Problem

Today `g o` opens the optimizer as a small pane listing candidate reorderings with latency scores. `Enter` previews a candidate (reorders sections, rebuilds graph, status "Preview: <label>"), but the user must manually inspect the graph, source, and module UI to see what changed. No cross-view focus exists.

## Solution

On click (or `Enter`) on a candidate row:
1. Preview the reorder (existing behavior)
2. Build two `SceneSpec` snapshots: `before` (file order) and `after` (candidate order)
3. Diff them at render time in the graph pane: per-node color by membership (before-only red, after-only blue, both purple, unchanged dim), per-cable dashed yellow when latency delta ≠ 0
4. Set `App.selected_circuit` to the first circuit whose section index changed
5. Jump source viewer to that circuit's first occurrence
6. Module UI highlights the corresponding hardware elements via existing `select_component` machinery
7. `Esc` restores original order and clears focus

## Non-Goals

- No side-by-side graph panes (single overlaid diff)
- No persistence of optimizer focus across patch loads
- No new config keys
- Optimizer pane stays a small-class pane; click does not close it

## Affected Components

- `src/app.rs` — optimizer state, preview logic, cross-view focus wiring
- `src/gui/graph.rs` — diff render path (dual SceneSpec + per-node/cable color logic)
- `src/gui/overlays.rs` — optimizer row click handling
- `src/handler.rs` — click routing to optimizer