# Tasks

## 1. Pure grouping model

- [x] 1.1 Create `src/graph_group.rs` with `GroupSettings { enabled, min_size }`, `Group { circuit, members, center, bbox }`, `GroupedView { groups, assignments, edges, drawn }` and `group_nodes(graph, positions, sizes, settings, expanded, isolated)`: union-find over same-type circuit nodes merged on drawn proximity or at most `GROUP_HOP_LIMIT` (2) cable hops, components at or above `min_size` become groups and the rest individuals, expanded groups keep their members drawn with an outline, an isolated group renders only its members plus dimmed 1-hop neighbours, merge order is ascending node index for determinism, and edges are aggregated and deduplicated; register `pub mod graph_group` in `src/lib.rs` <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/graph_group.rs, src/lib.rs] -->
- [x] 1.2 In-module `#[cfg(test)]` tests against a real fixture: a contiguous encoder stack collapses; wire-connected copies group at two hops; a component below the minimum stays individual; identical input yields an identical view; expand and isolate variants behave as specified; edges deduplicate and intra-group edges drop <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/graph_group.rs] -->

## 2. Configuration

- [x] 2.1 Add a `[graph]` table (`group_enabled: bool` default true, `min_group_size: usize` default 5 clamped 2..=64) to `Settings` in `src/config.rs` with defaults, clamping on load and save, and document it in `docs/config.md` <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/config.rs, docs/config.md] -->
- [x] 2.2 Tests: a missing table yields the defaults; a malformed `min_group_size` warns once and falls back; out-of-range values clamp to 2 and 64; save round-trips <!-- agent: horst-engineer.fast, depends_on: [2.1], touches: [src/config.rs] -->

## 3. Application state

- [x] 3.1 Add grouping state to `App`: `graph_grouping` seeded from config, `expanded_groups`, `isolated_group`, and a cached `grouped_view` invalidated on `open_graph`, `rebuild_graph`, a settings change, an expand and an isolate; seed the settings in `seed_app` <!-- agent: rusty-engineer.build, depends_on: [1.1, 2.1], touches: [src/app.rs, src/main.rs] -->
- [x] 3.2 Tests: the cache invalidates on rebuild and on each state change; expand and isolate toggles flip; the settings are seeded from config <!-- agent: horst-engineer.build, depends_on: [3.1], touches: [src/app.rs] -->

## 4. Rendering

- [x] 4.1 In `src/gui/graph.rs`, build the scene from the grouped view when grouping is enabled: one world-space box per collapsed group labelled `<circuit> ×<count>` in the circuit colour, member nodes plus a group outline for expanded groups, and aggregated edges, keeping the existing level-of-detail and ellipsis rules <!-- agent: frontend-engineer, depends_on: [1.1, 3.1], touches: [src/gui/graph.rs] -->
- [x] 4.2 Headless egui paint tests: a collapsed group paints one box and its label and no member nodes; an expanded group paints its members plus the outline; aggregated edges emit; an over-long label ellipsizes <!-- agent: frontend-engineer, depends_on: [4.1], touches: [src/gui/graph.rs] -->
- [x] 4.3 Pre-warm the grouped view in the paint entry point (`src/main.rs` passes it into `build_scene_spec_with_view`, re-exported from `src/gui/mod.rs`) so a scene build never recomputes clustering, and keep the four pre-grouping invariants honest now that grouping defaults on (`gui::self_test::tests::self_test_passes_on_the_demo_patch`, `gui::tests::center_graph_camera_centers_nodes_in_the_real_pane`, `regression::regression_graph_anchor_no_overlap_at_every_zoom`, `regression::regression_graph_anchor_lod_omits_text_below_threshold`) — disable grouping explicitly where a test pins ungrouped paint geometry, and say why in a comment <!-- agent: rusty-engineer.build, depends_on: [4.1], touches: [src/main.rs, src/gui/mod.rs, src/gui/self_test.rs, src/regression.rs] -->

## 5. Interaction

- [x] 5.1 In `src/handler.rs`, bind `o` (isolate), `Enter` (expand or collapse) and `Shift+O` (toggle grouping) in the graph-pane handler, route world-space group hover and click through the existing pointer path, and publish the hovered group for the painter <!-- agent: rusty-engineer.build, depends_on: [3.1, 4.1], touches: [src/handler.rs] -->
- [x] 5.2 Render the hover content peek in `src/gui/graph.rs`: the members' shared label prefix when one exists, otherwise the first eight labels plus `…+N` <!-- agent: frontend-engineer, depends_on: [5.1], touches: [src/gui/graph.rs] -->
- [ ] 5.3 Tests: each key dispatches; `o` filters the scene and refits the camera; `o` and `Enter` without a hover are silent no-ops; the popover text follows the peek rule <!-- agent: horst-engineer.build, depends_on: [5.1, 5.2], touches: [src/handler.rs, src/gui/graph.rs] -->
- [x] 5.4 Add the three rows to the graph help table in `src/help.rs` and to `docs/keys.md` <!-- agent: rusty-engineer.fast, depends_on: [5.1], touches: [src/help.rs, docs/keys.md] -->

## 6. Group-level highlights

- [x] 6.1 Map the existing highlight sources onto groups in the grouped view: selection, diff classification, topology errors, the influence wash and the dependency filter light a group when any member matches, under the topology-error > diff > influence > cable-kind precedence <!-- agent: frontend-engineer, depends_on: [4.1, 5.1], touches: [src/gui/graph.rs, src/app.rs] -->
- [ ] 6.2 Tests: a group lights when a member is selected, diffed, or carries a topology error; the precedence order holds <!-- agent: horst-engineer.build, depends_on: [6.1], touches: [src/gui/graph.rs] -->

## 7. Documentation and gates

- [ ] 7.1 Update `docs/views.md` with the grouping behaviour, then regenerate `ARCHITECTURE.md` and `DESIGN.md` with the project's skills <!-- agent: devops-engineer.build, depends_on: [5.4, 6.1], touches: [docs/views.md, docs/config.md, ARCHITECTURE.md, DESIGN.md] -->
- [ ] 7.2 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked` and `cargo run --release -- --self-test`, recording each exit status <!-- agent: devops-engineer.fast, depends_on: [5.3, 6.2], touches: [] -->
