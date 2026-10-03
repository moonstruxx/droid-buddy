# Design

## Context

See `proposal.md` for motivation. The graph minimap is currently drawn in `paint_polish()` (src/gui/graph.rs) via `minimap_layout()` which computes a `Minimap` struct containing the panel rect, viewport rect, and node rects — all in egui points relative to the graph canvas. The handler uses `handle_graph_mouse()` for graph pane mouse interactions, which operates on terminal cell coordinates (u16 column, row). The renderer publishes pane geometry via `pane_hit_rects` and node hit rects via `graph_node_rects`, converted through `to_cell_rect()`.

## Goals / Non-Goals

**Goals:**
- Click anywhere on the minimap panel → camera centers on that world position
- Minimap rect published each frame for hit-testing (like `pane_hit_rects`)
- Reuse existing `minimap_layout()` world→minimap mapping logic for coordinate conversion
- Works at any zoom level, preserves current zoom
- Help text documents the interaction

**Non-Goals:**
- Right-click or modifier+click on minimap (keep simple)
- Minimap drag-to-pan (click-only for v1)
- Any changes to minimap rendering/appearance
- Source viewer minimap changes (already has click handling but rect not published at runtime)

## Decisions

### 1. Coordinate conversion path

**Decision:** Convert minimap panel rect from egui points → terminal cells in renderer, store in `App.graph_minimap_rect`. In handler, convert mouse click (terminal cells) back to egui points relative to minimap panel, then use existing `minimap_layout` scale factors to map to world coordinates.

**Why:** 
- Single source of truth: renderer knows exact drawn position
- Handler already uses terminal cell coords for all hit-testing
- `minimap_layout` already computes the world→minimap transform (sx, sy, inner panel offset)

**Alternatives considered:**
- Pass minimap transform through App and do world→click in handler: couples handler to layout internals
- Do click→world in renderer during paint: wrong layer, handler owns mutations

### 2. Expose `minimap_layout` data to handler

**Decision:** `minimap_layout` returns `Minimap` which already has `panel`, `viewport`, `nodes`. Add a `transform` field with `(bx, by, sx, sy, ix, iy)` — the world bounds origin, scale factors, and inner panel origin — so handler can invert the mapping. Or: recompute in handler from `scene_bounds` + known constants (MINIMAP_SIZE, MINIMAP_PAD, 4px inset).

**Choice:** Add transform to `Minimap` struct. Cleaner than duplicating constants and logic.

### 3. Where to publish minimap rect

**Decision:** In `paint_scene_in` (or `paint_polish`), after `minimap_layout` returns `Some(minimap)`, convert `minimap.panel` to terminal cells via `to_cell_rect(egui::Rect::from_min_size(...))` using the canvas origin, store in `App.graph_minimap_rect`. Clear to `None` when no minimap.

### 4. Click handling location

**Decision:** In `handle_graph_mouse`, check minimap click BEFORE node hit-testing. If click in `graph_minimap_rect`, compute target world position, call `app.graph_camera = Some(camera_pan(...))` or similar, return early.

## Risks / Trade-offs

| Risk | Mitigation |
|------|------------|
| Minimap rect in terminal cells vs egui points mismatch | Use same `to_cell_rect` path as `pane_hit_rects`; test with headless render |
| Click on minimap border (outside inner panel) maps to clamped world | `minimap_layout` already clamps viewport to panel; use same clamped bounds |
| Handler needs access to scene bounds for transform | Store transform in `App` alongside rect, or recompute from `app.graph` + positions |
| Minimap not drawn when graph fits in viewport | `minimap_layout` returns `None` when `bw <= canvas.width() && bh <= canvas.height()`; handle `None` gracefully |