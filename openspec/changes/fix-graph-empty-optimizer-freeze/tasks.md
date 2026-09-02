# Tasks: fix-graph-empty-optimizer-freeze

## Task 1 — Fix optimizer screen freeze (droid_tui-hj8)

- [ ] Make `App::open_optimizer` and `App::optimizer_set_weight` non-blocking for interactive use: cap per-strategy SEARCH_STEPS for the initial open, or defer heavy strategies, ensuring open returns within a single frame while preserving determinism and banner/same-name constraints. Verify with `cargo test optimize::` and a manual `g o` smoke (no freeze) plus `cargo test` green.
  <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/optimize.rs, src/app.rs, src/handler.rs] -->

## Task 2 — Fix graph window empty (droid_tui-fxt)

- [ ] Ensure `render_graph` / `render_graph_kitty` / `graph_fit_camera` always produce visible nodes: handle degenerate area (negative pixel_size clamp), validate kitty scene has visible node rects before early-return, and fall through to box-drawing otherwise. Verify with `cargo insta test --check` and graph snapshots plus manual `g g` smoke showing nodes.
  <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/ui.rs, src/graph_render.rs, src/app.rs] -->

## Task 3 — Regression & visual validation

- [ ] Add/adjust unit and snapshot tests covering both fixes (optimizer budget/determinism, graph visibility on small area and kitty fallback) and ensure `cargo test` and `cargo insta test --check` pass; close beads issues when green.
  <!-- agent: horst-engineer.build, depends_on: [1, 2], touches: [src/regression.rs, src/optimize.rs, src/ui.rs] -->

