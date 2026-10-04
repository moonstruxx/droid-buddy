# Tasks: Optimizer Click → Cross-View Focus

## Task List

- [ ] 1.1 Add `OptimizerDiffState` to `App` with diff node/edge maps and focused circuit <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [ ] 1.2 Add theme tokens for diff colors (before/after/both/dim/changed) to all palettes <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/theme.rs] -->
- [ ] 1.3 Extend `optimizer_preview` to capture before_spec, compute diff, set cross-view focus <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->
- [ ] 1.4 Add `compute_optimizer_diff` helper comparing two SceneSpecs <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->
- [ ] 1.5 Wire `optimizer_close`/`Esc` to clear diff state and restore single render <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/app.rs] -->
- [ ] 2.1 Add `paint_diff_scene` in `gui/graph.rs` dual-render branch <!-- agent: api-engineer.build, depends_on: [1.2, 1.4], touches: [src/gui/graph.rs] -->
- [ ] 2.2 Implement per-node color mapping (Before=red, After=blue, Both=purple, None=dim) <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [ ] 2.3 Implement per-edge dashed yellow for latency delta ≠ 0 <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [ ] 2.4 Ensure diff render uses same camera/pan/zoom as single render <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [ ] 3.1 Add row click detection in `paint_optimizer` → message to handler <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/gui/overlays.rs, src/handler.rs] -->
- [ ] 3.2 Route optimizer row click to `App::optimizer_preview` (same as Enter) <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs] -->
- [ ] 4.1 Integration test: `g o` → click row → verify diff colors, source jump, module UI highlight <!-- agent: horst-engineer.build, depends_on: [2.3, 3.2], touches: [tests/] -->
- [ ] 4.2 Regression: all existing optimizer tests pass, no new config keys <!-- agent: horst-engineer.fast, depends_on: [4.1], touches: [src/] -->