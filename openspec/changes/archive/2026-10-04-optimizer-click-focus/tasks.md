# Tasks: Optimizer Click → Cross-View Focus

> As-built note (2026-10-04): implemented in latency-delta form per WIP
> commit 0c8e232, diverging from the dual-SceneSpec proposal: no
> `before_spec`/`after_spec` snapshots, no `compute_optimizer_diff`, no
> `paint_diff_scene`, no purple Both, no dashed yellow. Instead
> `OptimizerDiffState` holds the file-order per-edge latency baseline
> (`before_cable`/`before_latency`, captured on optimizer open),
> `optimizer_preview` refits the camera + `select_circuit(first_affected_node())`
> for cross-view focus, and `gui/graph.rs` colors edges/nodes red (worse) /
> blue (better) vs the baseline under the existing precedence. Items below
> marked [x] are covered in this as-built form; spec delta stays Proposed
> until the spec is rewritten to match.

## Task List

- [x] 1.1 Add `OptimizerDiffState` to `App` with diff node/edge maps and focused circuit (as-built: latency baseline `before_cable`/`before_latency`, app.rs:384) <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [x] 1.2 Add theme tokens for diff colors (before/after/both/dim/changed) to all palettes (as-built: `graph_node/edge_diff_before` red / `diff_after` blue in all palettes, theme.rs) <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/theme.rs] -->
- [x] 1.3 Extend `optimizer_preview` to capture before_spec, compute diff, set cross-view focus (as-built: baseline captured on open; preview refits camera + selects first affected node, app.rs:1408) <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->
- [x] 1.4 Add `compute_optimizer_diff` helper comparing two SceneSpecs (as-built: superseded — per-edge baseline compare in `first_affected_node`, app.rs:1467; no SceneSpec snapshots) <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->
- [x] 1.5 Wire `optimizer_close`/`Esc` to clear diff state and restore single render (as-built: `optimizer_restore` + `drop_optimizer_state` clear `optimizer_diff_state`; Esc closes via `close_focused_view`) <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/app.rs] -->
- [x] 2.1 Add `paint_diff_scene` in `gui/graph.rs` dual-render branch (as-built: superseded — single-render branch recolors by baseline delta, graph.rs:866ff) <!-- agent: api-engineer.build, depends_on: [1.2, 1.4], touches: [src/gui/graph.rs] -->
- [x] 2.2 Implement per-node color mapping (Before=red, After=blue, Both=purple, None=dim) (as-built: worse=red / better=blue node borders, no purple, graph.rs:1181ff) <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [x] 2.3 Implement per-edge dashed yellow for latency delta ≠ 0 (as-built: worse=red / better=blue solid edges, no dashed yellow, graph.rs:1146ff) <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [x] 2.4 Ensure diff render uses same camera/pan/zoom as single render (as-built: same camera; preview refits via `fit_graph_camera`) <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [x] 3.1 Add row click detection in `paint_optimizer` → message to handler (as-built: `optimizer_row_at` hit-test in window-frame path, handler.rs:300/2014ff) <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/gui/overlays.rs, src/handler.rs] -->
- [x] 3.2 Route optimizer row click to `App::optimizer_preview` (same as Enter) (as-built: click + Enter both call `optimizer_preview`, handler.rs:827/2035) <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs] -->
- [x] 4.1 Integration test: `g o` → click row → verify diff colors, source jump, module UI highlight (covered: `optimizer_preview_arms_baseline_and_focuses_affected_circuit` — open→baseline, Enter→preview+focus consistency, r→clear — plus `optimizer_row_at_maps_rows_and_rejects_chrome` for click hit-testing and the graph.rs worse/better color tests; handler.rs. This test caught a real bug: preview stole focus via `select_circuit`, breaking `r` — fixed by re-focusing the optimizer pane)
- [x] 4.2 Regression: all existing optimizer tests pass, no new config keys (1103 lib + 12 main + 5 perf-gate green; no config changes)