# Tasks

## 1. Layout solver — sections mode joint solve

- [x] 1.1 Add sections-mode constants and branch in `src/layout.rs`: `SLICE_K`, `SECTION_HOLE_K`, `DEFAULT_SLICE_K`, `DEFAULT_SECTION_HOLE_K`, and a `LayoutMode::Sections` variant in `LayoutMode` enum. Verify `cargo build --lib` clean. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/layout.rs] -->
- [x] 1.2 Implement slice-index computation from patch order in `src/layout.rs`: `slice_index(graph: &Graph) -> Vec<usize>` returning 0..N-1 per circuit node (controller/jack nodes get special handling). Verify with unit test on a small fixture graph. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/layout.rs] -->
- [x] 1.3 Implement section-hole anchor computation in `src/layout.rs`: given `Graph` with `clusters` (banner groups), compute fixed anchors as mean slice of member circuits per group. Expose `compute_section_holes(graph: &Graph, slice_index: &[usize]) -> Vec<Option<(f32,f32)>>` (None for nodes not in a cluster). Verify with unit test on a graph with 2 banner groups. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.2], touches: [src/layout.rs] -->
- [x] 1.4 Extend `run_iterations` with sections-mode branch in `src/layout.rs`: add slice spring (toward `slice_index * HORIZONTAL_SPACING`), section-hole spring (toward fixed anchor), cable springs (existing), size-aware repulsion (using `node_world_sizes`), optional weak cohesion. Gate behind `LayoutMode::Sections`. Verify by adding a test that a graph with 2 clusters settles with members clustered around their hole anchors. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.3], touches: [src/layout.rs] -->
- [x] 1.5 Wire `solve_graph_positions` in `src/app.rs` to dispatch to sections-mode solve when `layout_mode == Sections`. Verify `cargo test --lib` green. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.4], touches: [src/layout.rs, src/app.rs] -->

## 2. Section-hole anchors from banner groups

- [x] 2.1 Add `compute_section_holes` call in `src/graph.rs`/`Graph::build_from_patch` so holes are computed from banner groups at graph build time and stored on `Graph` (new field `section_holes: Vec<Option<(f32,f32)>>`). Verify holes match the design's mean-slice formula on a fixture with 2 banner groups. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.3], touches: [src/graph.rs] -->
- [x] 2.2 ~~Pass section holes to `solve_graph_positions`~~ **superseded by 3.8/3.9**: the build-time `section_holes` (`mean_slice × pitch`) is no longer what the solver uses for solved positions. Sections mode derives lane slots and per-(group,row) band anchors internally (`section_hole_anchors_lanes`), because lanes are now width-aware and wrapped. `Graph.section_holes` remains populated and its own tests pass; nothing should assert solved positions against it.

## 3. App state + sliders + Arrange button

- [x] 3.0 Wire `layout_slice_force`/`layout_section_force` through the solver (`run_iterations_with_forces`) so the sliders change the solve; `arrange_sections` honors the live values.
- [x] 3.1 Add sections-mode state to `src/app.rs` (`showing_sliders` + `S` toggle)
- [x] 3.2 Implement `Arrange` action (arrange_sections + Shift+`A`) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [3.1], touches: [src/app.rs, src/handler.rs] -->
- [x] 3.3 Ensure `c`/`Shift+c` centering works in sections mode (mode-agnostic; mutation-verified regression on a wide graph) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [3.1], touches: [src/app.rs, src/handler.rs] -->
- [x] 3.4 Align `config::DEFAULT_SECTION_FORCE` with `layout::DEFAULT_SECTION_HOLE_K` (was 0.05 vs 0.15 — the app's sections solve ran weaker than the tuned constant)
- [x] 3.5 **Fix inert size-aware repulsion** (found by test-writing): magnitude was `1/d²`-scaled independent of node size and the grid cell stayed at the fixed `REPULSION_RADIUS` (120) while node widths reach ~496 — so wide pairs were never considered and two 496-wide nodes overlapped by ~300. Now a linear penetration ramp (zero exactly at the required separation) with a graph-adaptive grid cell; force mode byte-identical (golden vector).
- [x] 3.6 **Fix the exploding sections seed** (found by measurement): sections mode reused the force-mode Sugiyama seed, whose `topological_depth` (n-pass Bellman-Ford) reaches ~245,266 on cyclic patches ⇒ a 19,621,280-wide seed the clamped integrator (≤2400 travel) cannot undo. Sections mode now seeds by patch order; scale guard added.
- [x] 3.7 File the pre-existing force-mode seam-explosion bug as bead `droid_tui-68g` (out of scope: `own_scale` still yields 19,621,296 in force mode).
- [x] 3.8 **Width-aware lanes that wrap into rows** (user decision): lane per circuit in patch order, lane width = `max(min(estimated_width, SECTIONS_LANE_MAX_NODE_W=240))`, lanes packed left→right and wrapped at `SECTIONS_ROW_TARGET_W=4000`; slice spring and section-hole anchors retargeted to lane slots. `own_scale` aspect 40:1 → 1.09.
- [x] 3.9 **Remove the residual overlap** (non-overlap is a hard requirement): stacked y-targets (in-lane stacking no longer collapses), per-(banner-group, row) hole anchors (no cross-row dragging), stack-aware row height, a bounded per-node tether (`SECTIONS_TETHER=60`), and a deterministic bounded `separate_overlaps` post-condition. `own_scale` worst penetration 79.19 → 1.05, zero pairs above tolerance, aspect 1.34.
- [x] 3.10 Clamp the drawn node width (and its spec-space hit extent) in sections mode only to `SECTIONS_LANE_MAX_NODE_W`, so a capped frame and its hit area agree and column/force rendering stays byte-identical.
- [x] 3.11 Window hit-test uses the mode-aware drawn sizes (`gui::drawn_world_sizes`), so hover can no longer reach past a capped sections frame.

## 4. GUI sliders panel + Arrange button

- [x] 4.1 Implement `paint_sliders_panel` in `src/gui/graph.rs` (+ `gui/mod.rs` paint-path hook) <!-- agent: layout-designer-engineer.build, depends_on: [3.1], touches: [src/gui/graph.rs] -->
- [x] 4.2 Add Arrange button in sliders panel <!-- agent: layout-designer-engineer.build, depends_on: [4.1, 3.2], touches: [src/gui/graph.rs] -->
- [x] 4.3 Wire panel pointer priority (panel eats pointer before node-drag/marquee) <!-- agent: layout-designer-engineer.build, depends_on: [4.1], touches: [src/gui/graph.rs, src/gui/mod.rs] -->

## 5. Config constants + graph-arrange cycle

- [x] 5.1 Add `[layout]` slice/section constants to `src/config.rs` (with defaults matching layout.rs) so they can be persisted later if needed. Verify `cargo build --lib` clean. <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/config.rs] -->
- [x] 5.2 Update `graph-arrange` cycle in `src/app.rs`/`src/handler.rs`: `a` cycles `column-strict → column-barycenter → force → sections`; `h` kept consistent. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.5], touches: [src/app.rs, src/handler.rs] -->

## 6. Tests + regression

- [x] 6.2 Sections-mode centering regression (mutation-verified, in `src/app.rs`) — covered by 3.3
- [x] 6.4 Sliders panel tests (render/binding/Arrange-click/marquee consumption) — covered by 4.1/4.3

- [x] 6.1 Add sections-mode regression story in `src/regression.rs` (`regression_sections_mode_clusters_around_section_holes`) — landed, then repaired under 6.5 after the lane redesign changed its contract. <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [2.2, 3.2, 4.2], touches: [src/regression.rs] -->
- [x] 6.3 Add layout unit tests in `src/layout.rs` for sections-mode (slice index, hole anchors, slice+hole springs, size-aware repulsion prevents overlap, stacked lanes, wrapped bands, own_scale scale guard). <!-- agent: horst-engineer.build, depends_on: [1.4], touches: [src/layout.rs] -->
- [x] 6.5 **Repair the 6.1 story** to the redesigned contract: lanes are width-aware and wrapped, members are tethered to their own lane slot (drift guard 100), anchors are per-(group,row), and cluster spans are no longer bounded by the inter-cluster gap.

## 7. Visual artifacts + verification

- [x] 7.1 Re-bless screenshots: two new shots (`docs/assets/sections-lanes.png` on `graph_topology_error.ini` for lane/section legibility, `docs/assets/sections-scale.png` on `own_scale.ini` for the aspect fix); every pre-existing PNG byte-identical; full `render.sh` check exit 0. <!-- agent: layout-designer-engineer.build, depends_on: [4.2, 6.1], touches: [tools/screenshots/**, docs/assets/*.png] -->
- [x] 7.4 **Suppress cluster boxes in sections mode** — an explicit spec requirement and scenario (`graph-sections-layout`: "No cluster boxes drawn in sections mode") that the task breakdown missed; caught by reviewing `sections-lanes.png`, where the enclosing rectangles were still drawn. One suppression point in `gui::graph::build_scene_spec` (`render_clusters && layout_mode != Sections`); the now-stale shots re-blessed; column/force scenes byte-identical (guarded).
- [ ] 7.2 Run full gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, `tools/screenshots/render.sh`. All exit 0. <!-- agent: horst-engineer.fast, depends_on: [6.1, 6.2, 6.3, 6.4, 7.1], touches: [] -->
- [x] 7.3 Beads: **no bead spawned this change** (it came from a user-requested explore→propose, not the ready queue), so there is nothing to close. The one follow-up this work produced — the pre-existing force-mode seed explosion — is already filed as `droid_tui-68g` (verified present, P2, open). <!-- agent: devops-engineer.fast, depends_on: [7.2], touches: [] -->