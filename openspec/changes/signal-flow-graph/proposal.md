## Why

DROID patches can be dense graphs of virtual-cable signal flow — working patches commonly have 600+ circuits. The current UI shows controller panels (physical hardware mapping) great for "which key", but it hides the wiring topology. Users need a view that reveals how circuits connect, to understand, debug, and re-purpose a patch.

## What Changes

- A new **optional signal-flow node-graph view** where circuits become nodes and DROID virtual cables (`output = _NAME` / params referencing `_NAME`) become edges, laid out by a force-directed solver that converges and freezes.
- Parse-time **virtual-cable extraction** in `patch.rs` (new signal-flow index, comment-aware, expression-embedded tokens captured).
- **Banner-range grouping**: comment banners (`# ---- Name ----`) own circuit ranges until the next banner or EOF; the graph renders these as cluster containers.
- **Topology validation**: `1 source → n sinks` is valid; `n → 1` is flagged. The graph highlights invalid wiring.
- **View-switch key** `g g` (fits the existing `g` prefix pattern) opens/closes the graph; `Esc` returns to the controller panels.
- The controller-panel view stays the primary surface (hardware key map); the graph is a separate, optional view into topology.

## Capabilities

### New Capabilities

- `signal-flow-graph`: the node-graph view and its layout/render/interaction loop.

### Modified Capabilities

- `patch-parsing`: **ADDED** requirement — virtual-cable/signal-flow extraction (new parse output alongside existing hardware-token extraction).
- `keybinding`: **ADDED** requirement — `g g` opens the signal-flow graph view (fits the existing prefix-key pattern for extensibility).

## Impact

- `src/patch.rs` — extended boundary-aware scanner to capture `_NAME` virtual cables from real section params (ignore comments, handle expression embedding); cable index stored on `Patch`; banner-range grouping logic.
- `src/graph.rs` — new GraphNode/GraphEdge model, topology validation (1→n vs n→1), graph build from cable index.
- `src/layout.rs` — convergence-based force-directed layout solver (spring force + friction, energy threshold → freeze, deterministic seed, spatial partitioning for 600+ nodes), re-solve on patch load or node move.
- `src/app.rs` — `showing_graph`, `graph` state field, positions, clusters.
- `src/handler.rs` — `g g` opens graph, node drag re-triggers solver, `Esc` closes, focus returns to panels.
- `src/ui.rs` — ComfyUI-style node frames, left/right ports, edge curves (box-drawing), color-coded cable types, cluster containers from banner groups; graph overlay atop the terminal.
- `src/events.rs` — observer-event bus (node moved, cable added, topology error) between model/graph/renderer.

## Non-Goals

- Continuous animated simulation (the solver converges then freezes — no drift, no tick).
- Replacing the controller-panel view (it stays primary; graph is overlay/supplementary).
- Persistence / hardware bridge (deferred; YAGNI).

## Verification

All implementation tasks include unit tests or integration verification (see tasks.md). The full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`) must pass.