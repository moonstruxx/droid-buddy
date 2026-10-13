# Tasks

## 1. Layout solver — sections mode joint solve

- [x] 1.1 Add sections-mode constants and branch in `src/layout.rs`: `SLICE_K`, `SECTION_HOLE_K`, `DEFAULT_SLICE_K`, `DEFAULT_SECTION_HOLE_K`, and a `LayoutMode::Sections` variant in `LayoutMode` enum. Verify `cargo build --lib` clean. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/layout.rs] -->
- [x] 1.2 Implement slice-index computation from patch order in `src/layout.rs`: `slice_index(graph: &Graph) -> Vec<usize>` returning 0..N-1 per circuit node (controller/jack nodes get special handling). Verify with unit test on a small fixture graph. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/layout.rs] -->
- [x] 1.3 Implement section-hole anchor computation in `src/layout.rs`: given `Graph` with `clusters` (banner groups), compute fixed anchors as mean slice of member circuits per group. Expose `compute_section_holes(graph: &Graph, slice_index: &[usize]) -> Vec<Option<(f32,f32)>>` (None for nodes not in a cluster). Verify with unit test on a graph with 2 banner groups. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.2], touches: [src/layout.rs] -->
- [x] 1.4 Extend `run_iterations` with sections-mode branch in `src/layout.rs`: add slice spring (toward `slice_index * HORIZONTAL_SPACING`), section-hole spring (toward fixed anchor), cable springs (existing), size-aware repulsion (using `node_world_sizes`), optional weak cohesion. Gate behind `LayoutMode::Sections`. Verify by adding a test that a graph with 2 clusters settles with members clustered around their hole anchors. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.3], touches: [src/layout.rs] -->
- [x] 1.5 Wire `solve_graph_positions` in `src/app.rs` to dispatch to sections-mode solve when `layout_mode == Sections`. Verify `cargo test --lib` green. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.4], touches: [src/layout.rs, src/app.rs] -->

## 2. Section-hole anchors from banner groups

- [x] 2.1 Add `compute_section_holes` call in `src/graph.rs`/`Graph::build_from_patch` so holes are computed from banner groups at graph build time and stored on `Graph` (new field `section_holes: Vec<Option<(f32,f32)>>`). Verify holes match the design's mean-slice formula on a fixture with 2 banner groups. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.3], touches: [src/graph.rs] -->
- [ ] 2.2 Pass section holes to `solve_graph_positions` and onward to `layout::run_iterations`. Verify the sections-mode test from 1.4 still passes. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [2.1, 1.5], touches: [src/graph.rs, src/app.rs] -->

## 3. App state + sliders + Arrange button

- [x] 3.0 Wire `layout_slice_force`/`layout_section_force` through the solver (`run_iterations_with_forces`) so the sliders change the solve; `arrange_sections` honors the live values.
- [x] 3.1 Add sections-mode state to `src/app.rs` (`showing_sliders` + `S` toggle)
- [x] 3.2 Implement `Arrange` action (arrange_sections + Shift+`A`) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [3.1], touches: [src/app.rs, src/handler.rs] -->
- [x] 3.3 Ensure `c`/`Shift+c` centering works in sections mode (mode-agnostic; mutation-verified regression on a wide graph) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [3.1], touches: [src/app.rs, src/handler.rs] -->
- [x] 3.4 Align `config::DEFAULT_SECTION_FORCE` with `layout::DEFAULT_SECTION_HOLE_K` (was 0.05 vs 0.15 — the app's sections solve ran weaker than the tuned constant)

## 4. GUI sliders panel + Arrange button

- [ ] 4.1 Implement `paint_sliders_panel` in `src/gui/graph.rs`: small egui panel at top-right of graph pane (toggleable via `showing_sliders`), three `egui::Slider` rows bound to `app.slice_force`, `app.section_force`, `app.tension`, each with label + range. Panel rect consumes pointer before node-drag/marquee/minimap (check `minimap` click pattern). Verify panel renders and sliders move the values. <!-- agent: layout-designer-engineer.build, depends_on: [3.1], touches: [src/gui/graph.rs] -->
- [ ] 4.2 Add Arrange button in sliders panel: `egui::Button` calling `app.arrange_sections()`. Verify click triggers re-solve and status update. <!-- agent: layout-designer-engineer.build, depends_on: [4.1, 3.2], touches: [src/gui/graph.rs] -->
- [ ] 4.3 Wire panel pointer priority: panel rect eats mouse before node-drag/marquee/minimap (same pattern as minimap click in `paint_scene_in`). Verify dragging a slider doesn't start node drag. <!-- agent: layout-designer-engineer.build, depends_on: [4.1], touches: [src/gui/graph.rs] -->

## 5. Config constants + graph-arrange cycle

- [x] 5.1 Add `[layout]` slice/section constants to `src/config.rs` (with defaults matching layout.rs) so they can be persisted later if needed. Verify `cargo build --lib` clean. <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/config.rs] -->
- [ ] 5.2 Update `graph-arrange` cycle in `src/app.rs`/`src/handler.rs`: `a` key cycles `Column-Strict → Column-Barycenter → Force → Sections`. Update `graph-arrange` spec delta test. Verify cycle order. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.5], touches: [src/app.rs, src/handler.rs] -->

## 6. Tests + regression

- [ ] 6.1 Add sections-mode regression test in `src/regression.rs`: load a multi-banner-group fixture (e.g., `pulsar_clock.ini` equivalent), open graph in sections mode, assert members cluster around section holes, cluster boxes not drawn, `c` centers, `A` re-solves. Verify `cargo test regression::` green. <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [2.2, 3.2, 4.2], touches: [src/regression.rs] -->
- [ ] 6.2 Add centering regression test for wide sections-mode graph (fixture with >20 circuits spanning width). Assert `c` centers horizontally. Verify `cargo test regression::` green. <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [3.3], touches: [src/regression.rs] -->
- [ ] 6.3 Add layout unit tests in `src/layout.rs` for sections-mode: slice index assignment, section hole computation, slice+hole spring forces, size-aware repulsion prevents overlap. Verify `cargo test layout::` green. <!-- agent: horst-engineer.build, depends_on: [1.4], touches: [src/layout.rs] -->
- [ ] 6.4 Add sliders panel tests in `src/gui/graph.rs`: panel renders, sliders bound to app state, Arrange button triggers re-solve, pointer priority. Verify `cargo test gui::graph::` green. <!-- agent: horst-engineer.build, depends_on: [4.2, 4.3], touches: [src/gui/graph.rs] -->

## 7. Visual artifacts + verification

- [ ] 7.1 Re-bless screenshots via `tools/screenshots/render.sh --bless` (sections mode shot + existing shots must stay byte-identical except new sections shot). Verify `render.sh` full check exit 0. <!-- agent: layout-designer-engineer.build, depends_on: [4.2, 6.1], touches: [tools/screenshots/**, docs/assets/*.png] -->
- [ ] 7.2 Run full gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, `tools/screenshots/render.sh`. All exit 0. <!-- agent: horst-engineer.fast, depends_on: [6.1, 6.2, 6.3, 6.4, 7.1], touches: [] -->
- [ ] 7.3 Close bead `droid_tui-533` (or new bead) with change reference if applicable. Verify with `bd list --json`. <!-- agent: devops-engineer.fast, depends_on: [7.2], touches: [] -->