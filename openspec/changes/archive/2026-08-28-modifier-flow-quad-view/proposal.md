## Why

DROID patches wire circuits via virtual cables (`_NAME` produced by `output = _NAME`, consumed by params like `input = _NAME`, `select = _NAME`, `selectat`). Today `droid_tui` exposes two separate views: controller panels (physical hardware) and a signal-flow graph (full topology, one at a time). Two gaps remain:

1. **Modifier tracing is manual.** A modifier is a hardware token (e.g. `B1.1`) that writes a variable (`output = _CLOCK`), which is later consumed by `select`/`selectat`. That consumption may be indirect — e.g. through a `switch` — so users must chase `_CABLE` mentions across the source to understand the influenced region. The existing `modifier_index` handles `select` transitivity for source highlighting, but the graph has no forward influence walk.

2. **Views are exclusive.** Picking a modifier in panels, inspecting its `select` sites in source, and seeing its graph impact requires view-switching (`g v` / `g g`). Debugging a patch wants all four signals concurrently: (1) which modifier is selected, (2) where its `select` sites live in source, (3) how its influence highlights inside the full graph, (4) what the isolated influenced subtree contains.

GPU-accelerated kitty rendering was proposed as polish but is not a hard requirement; herdr second-process hosting was considered and deferred.

## What Changes

### 1. Modifier influence trace (patch + graph)

- A **forward BFS influence walk** starting from the modifier's produced variable(s) (`_VAR` written by circuits that reference the selected hardware token and have `output = _VAR`).
- Walk rule: `cable -> sink circuits (any param consuming _VAR) -> if sink has output port(s) (i.e. `output = _X`) and is on the current flow, queue its output cables and continue`. Hop criterion is structural — **any circuit with input+output ports on the current signal flow**, not an allowlist — so `switch`, `copy`, `mix`, `logic`, etc. are handled uniformly.
- Cycle-safe (visited cables/circuits), deterministic. Result: `{ influenced_nodes: Set<NodeId>, influenced_edges: Set<cable> }`.
- `Patch::influence_subtree(var) -> InfluenceSubtree` (pure, no terminal dependency). `Graph::filtered_influence(...) -> Graph` (induced subgraph on influenced nodes/edges). Highlight sets travel with the graph for the renderer.

### 2. Quad concurrent view (app + handler + ui)

- **Four co-visible panes** in one terminal: top row `panels (modifier pick) | source (raw/prettified, select-highlight)`, bottom row `graph FULL (all nodes/edges, influenced path bold, rest dimmed) | graph FILTERED (only influenced subtree, freshly converged compact layout)`.
- In-process ratatui layout (extends the existing `viewer_split_ratio` horizontal split and the `header/main/status` vertical split). Responsive fallback below ~120 cols collapses gracefully (existing exclusive modes remain available).
- **Focus cycle** (`Tab`) across four panes; `Esc` closes quad and returns to panels; `g g` / `g v` remain valid; picker remains highest priority.
- New theme tokens: `graph_node_highlight`, `graph_edge_highlight`, `graph_edge_dim` (plus `graph_node_dim` if needed) — mono/terminal palettes stay pairwise distinct.
- **Kitty graphics** is an optional feature-flag (`kitty-gfx`) that replaces box-drawing edge/node rendering with an image when `KITTY_WINDOW_ID`/`TERM == xterm-kitty` is detected, otherwise falls back to box-drawing. No IPC, no second binary in this change.
- Herdr integration is deferred (would reverse the prior `ViewerMode::Herdr` removal); noted as a future experiment only.

## Capabilities

### New Capabilities

- `modifier-trace`: forward influence walk from a modifier's variable(s) through indirect hops; highlight sets; filtered subgraph derivation.
- `quad-view`: four-pane concurrent layout (panels, source, graph FULL, graph FILTERED), focus cycle, responsive fallback, feature-flagged kitty polish.

### Modified Capabilities

- `patch-parsing`: **ADDED** requirement — `influence_subtree` forward trace semantics and the `circuit -> output cables` reverse map.
- `signal-flow-graph`: **ADDED** requirements — highlight/dim rendering in FULL, filtered compact re-solve in FILTERED, influenced-edge color override.
- `theming`: **ADDED** requirements — highlight/dim tokens for graph nodes/edges.
- `viewer-layout`: **ADDED** requirement — quad-view layout extends the embedded source-pane split.
- `keybinding`: **ADDED** requirement — `Tab` cycles quad focus; `Esc`/`g g`/`g v` semantics in quad mode.
- `visual-validation`: **ADDED** requirements — gallery/snapshot coverage for quad + highlight + filtered scenarios.

## Impact

- `src/patch.rs` — `influence_subtree` walk, `circuit_outputs` reverse map, tests for switch-passthrough and copy chains.
- `src/graph.rs` — `InfluenceSubtree`, `filtered_influence`, `highlighted_nodes/edges` fields, induced-subgraph builder.
- `src/layout.rs` — filtered-subgraph `solve()` re-invocation (compact convergence); no API break to existing `solve`/`local_resettle`.
- `src/theme.rs` — new tokens `graph_node_highlight`, `graph_edge_highlight`, `graph_edge_dim`, etc.; palettes for `classic`/`mono`/`terminal`.
- `src/app.rs` — quad state (`showing_quad`, `quad_focus`, `active_modifier_var`, `influence: Option<InfluenceSubtree>`, `filtered_graph/positions`), `recompute_influence` on modifier selection, focus cycle.
- `src/handler.rs` — modifier selection drives `recompute_influence`; `Tab` cycles quad focus; `Esc` closes quad; graph drag in either graph pane triggers `local_resettle` + `NodeMoved`.
- `src/ui.rs` — quad layout (`Layout` splits for 2x2), FULL graph renders dim/bold by highlight set, FILTERED graph renders induced subgraph with its own compact positions, source pane continues to highlight `select` sites via `modifier_index`.
- `src/events.rs` — optional `InfluenceRecomputed` event (or reuse `GraphRebuilt`) to notify renderer/status.

## Non-Goals

- Kitty rendering is polish via feature-flag, not a hard window-system requirement. No second binary or Herdr pane in this change.
- No allowlist of "switch is special" — hop is structural (any input+output on path).
- No persistence or hardware bridge.
- No continuous animated simulation — both FULL and FILTERED graphs use the existing bounded convergence solver (freeze on energy threshold).

## Verification

- Unit tests: BFS walk (direct, indirect via switch, copy-chain, cycle-safe, leaf termination), filtered subgraph membership, highlight-set correctness.
- Layout tests: filtered solve converges and is finite; filtered positions differ from FULL (compact) when expected.
- Snapshot tests: quad-view frames × themes (`classic`/`mono`/`terminal`) × widths (80/120) × modifier-selected states; FULL highlight vs FILTERED compact.
- Visual validation gallery: new rows for `led_pairs`, `modifier_switch_passthrough` fixtures showing 4-pane concurrent rendering.
- Full gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo insta test --check`, `cargo build --release --locked`.
