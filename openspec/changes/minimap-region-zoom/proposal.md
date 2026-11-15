# Proposal

## Why

The graph minimap is a navigation aid, but it can only pan: clicking it recentres the camera at the current zoom, so reaching a dense corner of a big patch still takes a wheel-zoom plus several pans. The map was also suppressed whenever the layout fitted the viewport, so the one indicator of where the camera sits disappeared exactly when the graph was small enough to read.

## What Changes

- **The minimap is drawn whenever the graph has a scene**, not only when the world overflows the viewport. A fitting scene paints a clamped viewport box that reads as "the whole scene is visible".
- **Dragging a rectangle in the minimap zooms the graph to fit that region.** A press anchors a marquee instead of panning immediately; dragging updates it; the release either pans to the anchor point (a drag under 3 px on either axis — today's click, unchanged) or fits the camera to the world rectangle under the marquee and latches the fit zoom preset.
- **The marquee is visible while dragging** (translucent accent fill plus a 1 px stroke, above the dots and below the viewport box), `Esc` cancels it, and the canvas marquee selects nothing while the minimap owns the drag.
- **One implementation of the minimap↔world mapping** in `gui::graph` (`minimap_world_point`, `minimap_world_bounds`), consumed by both the pointer path and the painter instead of an inline inverse in the handler.
- **Help and docs** name the gesture (`drag minimap → zoom to dragged region`) and say the map is drawn at every zoom.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `signal-flow-graph`: a new requirement, "Minimap region zoom and always-visible map" — the always-on panel, the drag-to-fit gesture with its click threshold, the marquee's visibility and cancellation, and the suppression of the canvas marquee while the map owns the drag.
- `keybinding`: a new requirement, "Minimap drag gesture" — the mouse gesture and `Esc` cancellation, listed in the graph help table and the user manual.

## Impact

- `src/gui/graph.rs` — `minimap_layout` no longer suppresses the panel when the layout fits; new `minimap_world_point` / `minimap_world_bounds` / `MINIMAP_INSET`; `paint_minimap` draws the marquee.
- `src/gui/mod.rs` — re-exports the three minimap helpers for the handler.
- `src/handler.rs` — press anchors, drag updates, release decides; `MINIMAP_DRAG_MIN_PX`; shared `pan_minimap_to_world`; `Esc` cancels; camera fit through `GraphCamera::fit_to_world_with_nodes` plus the fit-preset latch.
- `src/app.rs` — `minimap_drag` state, cleared wherever the camera and hover state reset.
- `src/help.rs`, `docs/keys.md`, `docs/views.md`, `ARCHITECTURE.md` (ADR 59, ADR 39 annotated).

## Non-goals

- No new zoom math: the fit goes through the existing `GraphCamera::fit_to_world_with_nodes`.
- No keyboard equivalent for the gesture, and no persisted minimap state.
- No change to click-to-pan, node drag, canvas marquee selection, or the group press path.
- No minimap resizing or repositioning controls.
