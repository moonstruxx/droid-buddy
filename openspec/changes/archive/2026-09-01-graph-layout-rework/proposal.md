## Why

The signal-flow graph view lays out circuits as a striped-topology cloud seeded by banner-cluster bands that stack **vertically**, so on a wide terminal the graph is tall and underuses the available width. Cable springs are so weak that repulsion dominates — edges read as loose wires, not springs — the upstream source ("tip") is only *seeded* at the left and drifts freely, and manual node drags re-settle then snap back to spring equilibrium, so manual placements don't survive.

## What Changes

- **Single-axis pipeline layout**: seed the solver by topological layer on one dominant axis (depth → x, within-layer order → y) so the graph converges to a horizontal left→right chain instead of stacked cluster stripes.
- **Cable springs dominate repulsion**: raise spring influence so edges pull connected circuits into a readable chain.
- **Pinned tip**: the first circuit in `.ini` order is a fixed solver anchor at the left (the graph's "tip").
- **Cluster cohesion + enclosing rectangle**: banner groups render as enclosing rectangles with an internal cohesion force keeping members together (clusters as content containers, not layout stripes).
- **Width-first aspect-preserving fit**: the screen-fit preserves aspect ratio and prefers filling the canvas width, on both the box and kitty render paths.
- **Kitty-gfx default**: `kitty-gfx` joins **default features**; the kitty image renderer becomes the default, box-drawing the runtime fallback when the terminal doesn't support kitty (or the feature is off).
- **Manual pin + drag-to-place**: `p` pins/unpins a hovered node (pinned = fixed anchor); dragging pin-places a node so manual arrangements survive; unpin re-flows.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `signal-flow-graph`: modified requirements for single-axis convergence layout with cable-spring dominance and a pinned tip; banner-range grouping renders clusters as enclosing rectangles with member cohesion; kitty-graphics becomes the default graph renderer with box-drawing fallback; the pan/zoom initial fit preserves aspect and prefers filling width; and a new manual-pin / drag-to-place interaction.

## Impact

- **Code**: `src/layout.rs` (single-axis seed, spring/repulsion rebalance, pin anchors, cluster cohesion, camera-fit inputs), `src/graph.rs` (cluster/layout metadata), `src/ui.rs` (fit consumption, cluster rectangle rendering, kitty-default dispatch + box fallback), `src/graph_render.rs` (`GraphCamera` width-first fit), `src/app.rs` (pinned-set state, tip pin, drag-to-pin), `src/handler.rs` (`p` key, drag-to-pin), `Cargo.toml` (`kitty-gfx` in default features).
- **Schema / behavior**: parsing, validation, diff untouched; the graph model is unchanged except pin metadata.
- **Determinism**: unchanged (still no RNG; pins are deterministic inputs).
- **Dependencies**: no new external deps.

## Non-goals

- No change to parsing, validation, diff, or the patch model.
- No full layered-DAG rewrite beyond the single-axis bias; cluster content is retained as rectangles + cohesion, not removed.
- No animation or incremental re-layout beyond load/drag.
- No undo/history for manual pins.
- No persistence of manual pin positions across sessions (YAGNI).
