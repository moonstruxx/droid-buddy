# Tasks

Implemented before this record was written (the work went through `/plan-quick` and landed as commits); the checklist records what the implementation contains.

## 1. Geometry and painting

- [x] 1.1 Add `minimap_world_point` / `minimap_world_bounds` / `MINIMAP_INSET` beside `minimap_layout` in `src/gui/graph.rs`, with zero-scale and zero-span guards, and re-export them through `src/gui/mod.rs` <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/gui/graph.rs, src/gui/mod.rs] -->
- [x] 1.2 Drop the "layout fits the canvas, hide the map" early return and clamp the viewport box to the inner map rect so a fitting scene reads as fully visible <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/gui/graph.rs] -->
- [x] 1.3 Draw the drag marquee in `paint_minimap` (translucent accent fill plus 1 px stroke, above the dots, below the viewport box) <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/gui/graph.rs] -->

## 2. Gesture

- [x] 2.1 Add `minimap_drag` to `App` and clear it wherever the camera and hover state reset (`close_graph`, `close_view_state`, `build_graph_state`, `rebuild_graph`, `reset_graph_state`) <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [x] 2.2 Press anchors inside the panel (no camera move), drag frames update and clamp the second corner, release decides between click-to-pan and region fit (`fit_to_world_with_nodes` plus the fit preset latch) and always consumes the frame <!-- agent: rusty-engineer.build, depends_on: [1.1, 2.1], touches: [src/handler.rs] -->
- [x] 2.3 Suppress the canvas marquee while the map owns the drag, and cancel the marquee on `Esc` without closing the graph <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/handler.rs] -->

## 3. Tests

- [x] 3.1 Pure mapping tests: inverse round-trip, reversed corners, zero-scale guard, zero-span widening <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/gui/graph.rs] -->
- [x] 3.2 Gesture tests: region drag fits the expected camera and latches the preset; a sub-threshold drag still pans; a drag outside the panel clamps; a release without a drag is a no-op; `Esc` and close clear the drag <!-- agent: rusty-engineer.build, depends_on: [2.2, 2.3], touches: [src/handler.rs] -->
- [x] 3.3 Paint tests: the marquee is emitted only while dragging; a fitting scene now draws the map instead of hiding it <!-- agent: rusty-engineer.build, depends_on: [1.2, 1.3], touches: [src/gui/graph.rs] -->

## 4. Documentation and gates

- [x] 4.1 Add the gesture to the graph help table and `docs/keys.md`, and the always-on map plus the drag to `docs/views.md` <!-- agent: rusty-engineer.build, depends_on: [2.2], touches: [src/help.rs, docs/keys.md, docs/views.md] -->
- [x] 4.2 Record the decision as ADR 59 and annotate ADR 39's superseded visibility clause <!-- agent: lead, depends_on: [], touches: [ARCHITECTURE.md] -->
- [x] 4.3 Gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo run --release -- --self-test` <!-- agent: lead, depends_on: [3.1, 3.2, 3.3], touches: [] -->
