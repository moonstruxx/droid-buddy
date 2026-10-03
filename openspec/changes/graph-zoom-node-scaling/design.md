# Design

## Context

See `proposal.md` — Why. Today the graph scene builder (`build_scene`, `src/gui/graph.rs`) maps only node *positions* through `GraphCamera::world_to_pixel`; every `NodeSpec` carries the fixed pixel constants `GRAPH_WINDOW_NODE_W`/`_H` (200×80, `src/app.rs`). Paint derives corner radius from a fixed constant, port radius from the fixed spec height, label font from the fixed spec height, and cable strokes/arrows from a fixed `EDGE_WIDTH`.

Two facts shape the fix:

1. The column arrangement already reserves *world* geometry per node: `layout::estimated_widths(graph)` is the snapped per-node width the solver uses for its column blocks, and vertical slots are `VERTICAL_SPACING` (120) apart. That estimate is therefore the correct world width for a node body.
2. The fit camera is applied in two places with different math: `GraphCamera::fit_to_world(bounds, pixel_size, min_node_px)` and `graph_window_fit_camera(positions, viewport)`, which subtracts one fixed node frame in pixels from the viewport. `App::fit_graph_camera` calls `fit_to_world` directly. All three assume fixed-size nodes.

Pointer hit-testing (`handle_graph_window_frame`) currently converts the fixed pixel size back to world units by dividing by zoom (`nw = GRAPH_WINDOW_NODE_W / camera.zoom`).

## Goals / Non-Goals

**Goals:**
- Node geometry (frames, radius, borders, ports, titles), cable strokes/arrows, and cluster chrome are camera-projected world geometry.
- Node size is consistent with the space the layout reserved, so the column arrangement has no overlap at any zoom.
- Readability is preserved at extreme zoom-out through level of detail, and structure remains visible through minimum clamps.
- Fit and hit-testing agree with the drawn geometry at every zoom.

**Non-Goals:**
- Making the force arrangement overlap-free (it reserves no boxes; unchanged behavior there).
- Changing solver spacing constants, the zoom preset list, or any keybinding.
- Redesigning the minimap beyond keeping its node dots readable.
- Persisting or re-solving layout state — this is a rendering/geometry change only.

## Decisions

### 1. Node world size from the layout's per-node estimate

**Decision:** Expose a renderer-facing helper in the layout module — a fixed `NODE_WORLD_H` (≤ `VERTICAL_SPACING`) plus `node_world_sizes(graph) -> Vec<(f32, f32)>` whose width is `estimated_widths(graph)[i]`. The scene builder multiplies those by `camera.zoom`.

**Why:** the estimator is already the solver's contract for how much horizontal room each node owns. Reusing it makes non-overlap structural rather than tuned, and keeps a single source for node width. Vertical spacing 120 with an 80-unit body leaves a visible gap.

**Alternatives considered:**
- *Fixed world size for every node* — simplest, but the column block width varies per column, so a size large enough for long titles overlaps narrow columns.
- *Keep fixed pixel size and clamp the minimum zoom* — prevents zooming out at all, which is the opposite of the request.
- *Scale node size by zoom but keep the current 200 world width* — node size and spacing both scale, so the overlap ratio is scale-invariant; overlap would persist at every zoom.

### 2. Scale all scene geometry with minimum clamps

**Decision:** multiply node radius, border width, port radius, cable width, arrow dimensions, and cluster padding/title size by the zoom, each with a minimum (border ≥ ~0.5 px, cable ≥ ~0.75 px, arrow has its own floor). Interaction chrome that is not scene geometry — the selection outline, marquee rectangle, hover tooltip, minimap panel — stays fixed size.

**Why:** at 0.1 zoom a 2 px cable would otherwise be thicker than the node it connects; at 8× zoom a fixed 2 px cable would be a hairline. Minimums keep the structure legible at the zoom floor.

**Alternatives considered:** scaling nodes only (cables become visually dominant when zoomed out); scaling without minimums (geometry vanishes at the floor).

### 3. Level of detail: fit titles to frames, then omit

**Decision:** compute the label font as the smaller of the frame-derived size and a width-derived size (monospace advance ≈ 0.6 × font, so `font ≤ w / (0.6 × chars)`), ellipsize when even the minimum font cannot fit, and skip labels, ports, and cluster titles entirely when the frame height is below a legibility threshold. Node frames and cable strokes get a minimum *render* size (about 1 px) applied at paint time, so geometry never vanishes at the zoom floor.

**Why:** with per-node widths, long titles (including `LabelStore` overrides, which the estimator does not see) no longer fit the old font ratio. A zoomed-out graph must not be a wall of overlapping text. The 1 px render floor is deliberately tiny: it only prevents geometry from disappearing, so the non-overlap guarantee is stated for the fit zoom and above — below the floor, frames converge to dense marks and overlap is visually meaningless.

**Alternatives considered:** ellipsize only (text becomes unreadable dots); keep a fixed font (defeats zoom-out).

### 4. Extent-aware fit, one API

**Decision:** add `GraphCamera::fit_to_world_with_nodes(bounds, node_world, pixel_size, min_node_px)`, which expands the world bounds by one node extent and fits `zoom = min(V_w / (span_w + node_w), V_h / (span_h + node_h))`. The `floor_frames` guard is preserved; note that this guard algebraically implies `min_node_px / node_h ≤ fit_zoom`, so the floor parameter can never *raise* the zoom above the pure fit — it is retained for API parity, and the minimum drawn node size is enforced at render time instead. Route `graph_window_fit_camera` through the same math instead of subtracting a fixed pixel frame, and update `App::fit_graph_camera` and `center_graph_camera` (which gains a half-node offset). `fit_to_world` is retained unchanged as the fixed-node compatibility wrapper until every caller migrates.

**Alternatives considered:** changing `fit_to_world`'s signature in place (breaks the crate mid-migration, so parallel waves could not compile or test); keep the pixel-subtraction heuristic (wrong once node size depends on zoom — it becomes circular); add a separate fit function for the window path (two sources of truth).

### 5. Hit-testing in world units with a minimum hit size

**Decision:** test pointer positions against the node's world rect (its solved position plus `node_world_sizes`), removing the `/ zoom` conversion, and expand the hit rect to a minimum pixel size when the drawn frame is smaller.

**Why:** the drawn frame *is* the world rect projected through the camera, so hit-testing in world space is exact at every zoom and simpler. The minimum keeps tiny zoomed-out nodes clickable without changing what is drawn.

### 6. Constants co-located with their consumers

**Decision:** node world height lives beside the width estimator in `src/layout.rs`; the pixel-constant names in `src/app.rs` are redefined as world units and the fit floor is documented as a node-pixel floor. No new config keys.

## Risks / Trade-offs

| Risk | Mitigation |
|---|---|
| Long `LabelStore` title overrides exceed the estimated world width | Title fit-to-frame (decision 3) shrinks then ellipsizes; the frame never grows beyond the reserved slot. |
| Filtered (dependency/influence) renders use subset indices | Widths are computed from the full graph and indexed through the same full-graph mapping the scene builder already uses. |
| `NODE_WORLD_H` (80) close to `VERTICAL_SPACING` (120) leaves a thin gap at zoom-out | Gap scales with zoom, so it stays proportional; the 1 px render floor keeps rows distinguishable down to the preset floor. |
| At the lowest zoom presets the 1 px render floor binds and frames may touch | Accepted: the spec states non-overlap for the fit zoom and above; below the floor the graph is a dense structure by design. |
| Force arrangement still overlaps (no reserved boxes) | Documented non-goal; the column arrangement is the default. |
| Existing shape/fit assertions encode fixed pixel sizes | Each implementation task updates its own file's assertions; the regression task covers the cross-layer behavior. |
| Minimap dots become sub-pixel when zoomed far out | `minimap_layout` already clamps dot dimensions to ≥ 1 px; the paint task raises the floor to ≥ 2 px. |

## Open Questions

None — the spec, approach, and task breakdown are settled by the decisions above.
