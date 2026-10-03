# Proposal

## Why

Graph zoom currently transforms node positions only. Node frames are fixed pixel constants (`GRAPH_WINDOW_NODE_W`/`_H` = 200×80), so zooming out shrinks the spacing while the frames stay full size — the graph collapses into a field of overlapping boxes — and zooming in spreads undersized frames far apart. Node-graph editors scale the node geometry with the camera, and the reported behavior ("in graph view we zoom in and out but we do not resize the graph nodes") is exactly this gap.

## What Changes

- Node bodies SHALL be sized in world units and projected through the camera: pixel size = world size × zoom. World width comes from the layout's existing per-node width estimate (the same estimate the column arrangement reserves), so a node's drawn frame always fits the space the solver gave it and the column arrangement never overlaps at any zoom.
- Node corner radius, border width, port markers, cable stroke width, direction arrows, and cluster padding/title SHALL scale with zoom, each with a minimum clamp so structure stays visible at the zoom floor and does not read as hairlines at high zoom.
- Labels SHALL be fitted to their node frame (font shrunk to fit, ellipsized when below the legible minimum) and omitted, together with port markers and cluster titles, below a legibility threshold. Node frames SHALL keep a small minimum pixel size so they never vanish.
- The fit camera SHALL frame the world bounds plus one node's world extent, so a fully zoomed-out fit keeps whole node bodies inside the pane rather than only their center points.
- Pointer hit-testing SHALL use the same world-space node extents the renderer draws (zoom-independent), with a minimum hit size so nodes stay selectable when their drawn frame is very small.

## Capabilities

### New Capabilities
- `graph-node-scaling`: world-space node geometry projected through the graph camera, zoom-proportional rendering of node/cable/chrome geometry, level-of-detail thresholds, extent-aware fit, and zoom-aligned hit-testing.

### Modified Capabilities
- `signal-flow-graph`: "ComfyUI-style rendering" gains world-sized, zoom-scaled node frames and title fitting; "Pan and zoom navigation" gains node-geometry scaling, level of detail, and an extent-aware fit.
- `gpu-graph-window`: "Canvas polish" pan/zoom now scales node geometry and keeps hit-testing aligned at every zoom.
- `mouse-interaction`: adds graph node hit-testing that follows the zoomed geometry.

## Impact

**Affected code:** `src/layout.rs` (world-size API), `src/graph_render.rs` (fit API), `src/app.rs` (node-size constants, fit/center helpers), `src/gui/graph.rs` (scene build, paint, level of detail, minimap, fit helper), `src/gui/mod.rs` and `src/main.rs` (fit call sites), `src/handler.rs` (hit-testing), `src/regression.rs` (regression coverage).

**Tests:** graph shape/fit assertions in `src/gui/graph.rs`, `src/app.rs`, `src/graph_render.rs`, and `src/handler.rs` move to the new semantics; a new cross-layer regression covers non-overlap at every zoom preset, label level of detail, and hit-testing at low and high zoom.

**No config, dependency, data-format, or keybinding changes.** `ARCHITECTURE.md` and `DESIGN.md` are derived artifacts and are regenerated at archive.
