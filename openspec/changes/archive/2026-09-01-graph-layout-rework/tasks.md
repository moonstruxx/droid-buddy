# Tasks

## 1. Solver

- [x] 1.1 Rework the force-directed solver: seed by topological layer on one dominant axis (depth → x, within-layer order → y) instead of stacked cluster bands; raise cable-spring dominance over repulsion; add a weak per-cluster cohesion force toward each cluster's centroid; add a `pinned: &[usize]` fixed-anchor parameter to `solve()`/`local_resettle()` (pinned nodes never move). Verify: `cargo test src::layout` passes, including new single-axis / spring-dominance / cohesion / pinned-anchor unit tests. <!-- agent: api-engineer.build, depends_on: [], touches: [src/layout.rs, src/graph.rs] -->

## 2. Render

- [ ] 2.1 Make the graph fit and cluster rendering reflect the new layout: compute a width-first aspect-preserving fit in `GraphCamera` (world→screen and box-drawing path) that prefers filling the canvas width; render each banner group as an enclosing rectangle (union of member node rects + padding) instead of a stiff vertical band; move `kitty-gfx` into default features and dispatch kitty-by-default with box-drawing as the runtime fallback when unsupported. Verify: `cargo test src::ui` passes, including render-fit and kitty-default dispatch tests, and `cargo build --release --locked` compiles with default features. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/ui.rs, Cargo.toml, src/graph_render.rs] -->

## 3. State and interaction

- [ ] 3.1 Wire pin state and interaction: add a pinned-set to `App` (NodeId-keyed, mapped to solver indices at solve time); pin the first circuit (`.ini` order) as the tip by default; add the `p` key to toggle pin/unpin on the hovered graph node (mirroring the `x` toggle pattern); auto-pin a node at the dropped position when it is dragged. Verify: `cargo test src::app src::handler` passes, including tip-pinned-by-default, `p` toggle, and drag-to-pin behavior tests. <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs, src/handler.rs] -->

## 4. Tests

- [ ] 4.1 Add solver regression coverage: single-axis convergence, spring-dominance over repulsion, cluster-cohesion (members cohere, no vertical-stripe revert), and pinned-anchor (tip fixed, dragged node stays, unpin re-flows) tests over real fixtures. Verify: `cargo test` passes with the new graph-layout regression tests in `regression.rs`. <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/layout.rs, src/graph.rs, regression.rs] -->

- [ ] 4.2 Add render-fit and interaction coverage: width-first aspect-preserving fit on both render paths, kitty-default (default feature set emits kitty, `--no-default-features` falls back to box-drawing), and drag-to-pin behavior through `handle_event`/`render`. Verify: `cargo test` passes with the new ui/handler regression tests. <!-- agent: horst-engineer.build, depends_on: [2.1, 3.1], touches: [src/ui.rs, src/handler.rs, src/app.rs, regression.rs, Cargo.toml] -->

## 5. Verification gate

- [ ] 5.1 Run the full verification gate and fix any failures: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`, and `cargo insta test --check`. Verify: all five exits are 0. <!-- agent: devops-engineer.fast, depends_on: [4.1, 4.2], touches: [] -->
