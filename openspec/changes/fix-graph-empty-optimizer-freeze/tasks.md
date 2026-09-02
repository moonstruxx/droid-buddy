# Tasks: fix-graph-empty-optimizer-freeze

## Task 1 — Fix optimizer screen freeze (droid_tui-hj8)

- [x] Make `App::open_optimizer` and `App::optimizer_set_weight` non-blocking for interactive use: capped per-strategy SEARCH_STEPS for initial open via `generate_candidates_weighted_fast` (INTERACTIVE_SEARCH_STEPS 250, hash-free EvalCtx), ensuring open returns within a single frame while preserving determinism and banner/same-name constraints.
  <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/optimize.rs, src/app.rs, src/handler.rs] -->

## Task 2 — Fix graph window empty (droid_tui-fxt)

- [x] Ensure `render_graph` / `render_graph_kitty` / `graph_fit_camera` always produce visible nodes: clamped avail_w/h to 1.0 in `graph_fit_camera`, defensive pixel_size clamp in `GraphCamera::fit_to_world`, and rect validation before kitty early-return to fall through to box-drawing.
  <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/ui.rs, src/graph_render.rs, src/app.rs] -->

## Task 3 — Regression & visual validation

- [x] Verified `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked` (810 passed), `cargo build --release --locked`, and `cargo insta test --check` (no snapshots to review) all green.
  <!-- agent: horst-engineer.build, depends_on: [1, 2], touches: [src/regression.rs, src/optimize.rs, src/ui.rs] -->

