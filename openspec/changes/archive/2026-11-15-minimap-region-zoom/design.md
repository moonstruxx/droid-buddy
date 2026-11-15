# Design

## Context

`minimap_layout` already publishes everything the gesture needs: the panel rect, each node's scaled frame, the viewport box, and `transform = (bx, by, bw, bh, ix, iy, sx, sy)` whose inverse maps a minimap point to world coordinates. The handler already inverted that transform inline for click-to-pan; the painter already had a `paint_minimap` pass. So the change is a state machine over an existing transform plus one shared mapping function, not new geometry.

## Goals / Non-Goals

**Goals**
- Dragging in the minimap lands the camera on exactly the world region under the marquee.
- A click keeps behaving exactly as before.
- One implementation of the minimap↔world mapping.

**Non-Goals**
- No new zoom curve, no re-solve, no change to node dragging or selection.

## Decisions

### D1 — The map is always drawn

The "layout fits the viewport, so hide the map" early return is removed. The viewport box is clamped to the inner map rect, so a fitting scene paints a full-map box ("everything is visible") instead of a degenerate line, and zoomed-in scenes are unchanged. Consequence, accepted: the pane's bottom-left corner is map, so a click there navigates rather than selecting whatever node sits under it.

### D2 — Press anchors, release decides

A press inside the panel stores `minimap_drag = Some((x, y, x, y))` in window points and consumes the frame without moving the camera. Drag frames update the second corner, both clamped to the inner map rect. The release compares the marquee span against `MINIMAP_DRAG_MIN_PX` (3.0) on either axis:

- under it → today's behaviour, a pan to the anchor's world point with the same status message (a click is simply a zero-size drag, so there is no second code path to keep in sync);
- at or above it → `GraphCamera::fit_to_world_with_nodes(world_bounds, graph_fit_node_world(), canvas_px, 0.0)`, then the same bookkeeping `fit_graph_camera` does (fit zoom preset latch), with a status message naming the region's world span.

*Alternatives considered.* A separate click handler plus a drag handler would duplicate the panel test and the mapping; and a drag threshold in pixels rather than on either axis would let a nearly-vertical flick zoom to a two-node-wide sliver. Rejected.

### D3 — One mapping, in `gui::graph`

`minimap_world_point` (inverse of the layout's `map`, zero-scale guarded) and `minimap_world_bounds` (two corners normalised, zero span widened to one world unit about its midpoint) live beside `minimap_layout`; the handler consumes them through `pub(crate)` re-exports. The handler's inline inverse is deleted, so the transform is inverted in exactly one place. The bounds widening matters because `fit_to_world_with_nodes` has a `MIN_SPAN` floor: a marquee one pixel tall would otherwise fit a mathematically degenerate rectangle.

### D4 — The marquee never reaches the canvas

While `minimap_drag` is `Some`, the frame belongs to the map: the canvas marquee is skipped (`commit_window_marquee` returns early), nodes never see the drag, and the release is consumed. Without this the same drag would both zoom and box-select.

### D5 — `Esc` cancels, state resets with the camera

`Esc` clears `minimap_drag` in the pane's existing focus-routing key step and leaves the view open. The drag is also cleared in `close_graph`, `close_view_state(Graph)`, `build_graph_state`, `rebuild_graph` and `reset_graph_state` — the same places that clear the camera and hover state, so a stale marquee can never outlive the graph it was drawn on.

## Risks / Trade-offs

- **[Risk] The always-on map swallows clicks in the pane's bottom-left corner.** → Accepted as inherent to the request; a click there navigates (the previous behaviour of that corner was node selection, which is still reachable by any drag or by panning first).
- **[Risk] A tiny drag zooms to a sliver.** → The 3 px floor on either axis routes it to click-to-pan instead, and the world bounds widen a zero axis.
- **[Risk] Fitting a very small region over-zooms past legibility.** → The fit is the pure fit (`min_node_px = 0.0`), which is the same rule the `Shift+c` fit uses; the zoom presets and wheel still bound what the operator sees.

## Migration Plan

No persisted state, no config key, single binary. Rollback is the reverse commit; the map becoming conditional again is a one-line early return.

## Open Questions

None.
