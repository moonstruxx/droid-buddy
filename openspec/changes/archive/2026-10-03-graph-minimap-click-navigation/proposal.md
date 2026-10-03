# Proposal

## Why

The signal-flow graph view has a minimap (drawn at the bottom-left of the graph pane) that shows the full graph layout with a viewport indicator box showing the currently visible area. Currently, this minimap is read-only — users must use arrow keys, `c`/`Shift+c`, or mouse wheel to navigate. Clicking the minimap to jump the camera to a specific location is a standard UX pattern in node-graph editors (ComfyUI, Blender, Unreal) and would significantly improve navigation speed on large patches.

## What Changes

- Add click handling to the graph minimap: left-click anywhere on the minimap panel pans the camera so the clicked world position becomes the new viewport center
- Publish the minimap panel rect from the renderer to `App` so the handler can hit-test clicks
- Extend the existing minimap layout logic to provide the world→minimap coordinate mapping for click-to-world conversion
- Update help text to document the minimap click navigation

## Capabilities

### New Capabilities

- `graph-minimap-navigation`: Click-to-navigate on the signal-flow graph minimap

### Modified Capabilities

- `signal-flow-graph`: Adds minimap click navigation as an interaction mode
- `mouse-interaction`: Adds graph minimap click as a mouse interaction target (parallel to source viewer minimap click)

## Impact

**Affected code:**
- `src/app.rs` — new field `graph_minimap_rect: Option<Rect>`
- `src/gui/graph.rs` — `paint_polish` publishes minimap rect; `minimap_layout` exposes world→minimap mapping
- `src/handler.rs` — `handle_graph_mouse` handles click on minimap, converts to world coords, pans camera
- `src/help.rs` — add minimap click to graph view keybindings

**No schema, dependency, or data format changes.**