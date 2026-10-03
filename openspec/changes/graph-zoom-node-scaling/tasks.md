# Tasks

## 1. World geometry and camera math

- [ ] 1.1 Add world-space node sizing to the layout model: a `NODE_WORLD_H` constant (≤ `VERTICAL_SPACING`) and `node_world_sizes(graph)` whose widths come from `estimated_widths`; verify with a unit test asserting each width equals the estimator and never exceeds its column pitch <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/layout.rs] -->
- [ ] 1.2 Extend `GraphCamera::fit_to_world` to take the node world extent and frame whole node bodies (node-pixel floor reinterpretation, existing `floor_frames` guard preserved); update the in-file fit tests and verify `cargo test --lib graph_render` passes <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/graph_render.rs] -->
- [ ] 1.3 Redefine the graph node-size constants as world units and the fit floor as a node-pixel floor in `src/app.rs`; update `App::fit_graph_camera` and `center_graph_camera` (half-node offset) to pass node extents and verify the app fit tests pass <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/app.rs] -->

## 2. Scene geometry and paint

- [ ] 2.1 `build_scene`: size node w/h as world size × camera zoom; scale radius, border width, port markers, cable width, arrow dimensions, and cluster padding/title with minimum clamps; add a spec-level test asserting node pixel size scales with zoom <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/gui/graph.rs] -->
- [ ] 2.2 `paint_scene_in` / `paint_arrow` / `paint_minimap`: apply the minimum clamps, fit labels to the frame (shrink font, then ellipsize), omit labels, port markers, and cluster titles below the legibility threshold, and raise minimap node dots to a ≥ 2 px floor; verify the shape/label tests pass <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/graph.rs] -->
- [ ] 2.3 `graph_window_fit_camera` passes the node world extent through the shared fit math; update the fit call sites in `src/main.rs` and `src/gui/mod.rs` and the fit tests; verify `cargo test --lib gui::graph` passes <!-- agent: api-engineer.build, depends_on: [1.2, 2.1], touches: [src/gui/graph.rs, src/gui/mod.rs, src/main.rs] -->

## 3. Interaction

- [ ] 3.1 `handle_graph_window_frame`: hit-test against world-space node extents (drop the `/ zoom` conversion) plus a minimum pixel hit size; update the `seed_graph_camera` literal; verify handler tests cover clicks at zoom 0.1 and zoom 2.0 <!-- agent: rusty-engineer.build, depends_on: [1.2, 1.3], touches: [src/handler.rs] -->

## 4. Regression coverage

- [ ] 4.1 Cross-layer regression over the scale anchor: no node overlap at every zoom preset on the column arrangement, labels omitted below the legibility threshold, and hit-testing at low and high zoom; verify `cargo test` passes <!-- agent: horst-engineer.build, depends_on: [2.1, 2.2, 2.3, 3.1], touches: [src/regression.rs] -->

## 5. Verification

- [ ] 5.1 Four gates all exit 0: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` <!-- agent: devops-engineer.fast, depends_on: [4.1], touches: [] -->
- [ ] 5.2 Live-screen proof: load the scale-anchor patch, open the graph, capture fit / two zoom-out steps / zoom-in showing node frames shrinking and labels dropping out, and compare against the pre-change captures <!-- agent: devops-engineer.fast, depends_on: [5.1], touches: [] -->
