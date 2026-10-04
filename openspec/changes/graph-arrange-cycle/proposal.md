# Proposal

## Why

Optimizer preview (`Enter`/click in the `g o` pane) re-solves the signal-flow graph, but the result sometimes renders as a startup-like layout; clicking a node then snaps it to its correct position. The manual escape hatch (`h` toggles column/force, `Shift+c` refits) exists but no single key re-applies the arrangement, so users are stuck when the automatic refit silently skips.

## What Changes

- Fix pin-anchor carry-over in `solve_graph_positions` to be reorder-safe (NodeId-based, not index-based), so optimizer reorders no longer scramble pinned anchors.
- Make optimizer preview refit unconditional against the live graph-pane rect (no silent skip when the published canvas size is stale/`None`).
- Add a graph-pane `a` key: applies the current arrangement (rebuild + refit); repeated presses cycle `Column-Strict → Column-Barycenter → Force → …`, with a status line naming the active arrangement.
- Document the key in the graph help table.

## Capabilities

### New Capabilities

- `graph-arrange`: manual arrangement trigger + arrangement-cycle + reliable post-rebuild refit contract.

### Modified Capabilities

- `signal-flow-graph`: arrangement is reorder-safe and refit is unconditional (behavior fix, no new UI besides `a`).
- `keybinding`: new graph-pane `a` binding + status/edge behavior.

## Impact

- Touched code: `src/app.rs` (anchor mapping, arrange-cycle state, preview refit), `src/handler.rs` (graph-pane `a` arm), `src/help.rs` (graph key table), `src/layout.rs` (consumer only, no solver change expected).
- No new dependencies, no config keys, no persistence, no `.ini` mutation.

## Non-goals

- No new layout algorithms beyond the existing column (strict/barycenter) + force paths.
- No change to drag/pin semantics, settle animation, or optimizer candidate generation.
- No global binding: `a` is graph-pane-only by explicit scope decision.
- No ARCHITECTURE.md/DESIGN.md hand-edits (derived artifacts, regenerated via /make-*).

## New types (YAGNI consumer check)

- `ArrangeCycle` (or equivalent arrangement selector state on `App`): consumed by `App::apply_arrangement` and the `handler.rs` `a` arm; no other new types/variants planned.
