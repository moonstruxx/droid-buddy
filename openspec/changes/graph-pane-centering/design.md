# Design: graph pane centering, fit, and deeper zoom-out

## Context

The graph camera today has three defects that together make the graph hard to inspect in a tile:

1. The first-frame fit is seeded in `main.rs` `RedrawRequested` (src/main.rs:211-227) with a hardcoded 1280×800 viewport via `gui::graph_window_fit_camera`. The graph slot in the tiled layout is a sub-rect of the window (the right column, width `(1 − main_split_ratio)` of the window), so the fit frames a region that does not match what the user sees. The same 1280×800 constants (`WINDOW_FIT_VIEWPORT_W/H`, src/gui/graph.rs:241-242) are baked into `graph_window_fit_camera`.
2. `App.graph_canvas_px` is the "visible pane size" handoff that `graph_canvas_center_world` (the zoom anchor, src/app.rs:1965) and `graph_pan_if_overflow` (the arrow-pan gate, src/app.rs:2008) read, but nothing in the production paint path ever writes it. Only tests set it (e.g. src/handler.rs:2605, 2662, 2691). So zoom steps anchor at the world origin `(0,0)`, which empties the pane when the content has drifted, and arrow-pan gating is dead.
3. Zoom-out is capped at the `0.0625` preset (`GRAPH_ZOOM_PRESETS`, src/app.rs:1958). The requested behavior is at least 2× deeper zoom-out from the fitted zoom.
4. `c` on the graph surface toggles latency coloring (src/handler.rs:964-969), leaving no key for centering.

The change is confined to the camera/keys around the graph surface. See proposal.md for motivation; the requirements live in the `signal-flow-graph` and `keybinding` deltas.

## Goals / Non-Goals

**Goals:**

- `c` centers the graph in the visible pane (pan only, zoom unchanged). `Shift+c` refits and centers (replace zoom). Latency coloring moves to `g c`.
- Fit, centering, zoom anchoring, and arrow-pan gating all use the visible pane's real size, published by the paint path each frame (the `component_rects` handoff pattern, ADR 4/22).
- Zoom-out reaches at least 2× below the fitted zoom: the preset list gains a `0.03125` step below `0.0625`.

**Non-Goals:**

- No mouse-gesture centering (middle-click or double-click).
- No camera persistence or camera config; the camera stays session state.
- No changes to the physical view's zoom/pan.
- No latency coloring semantics changes, only its key chord.
- No layout-solver changes.

## Decisions

### D1: Publish `graph_canvas_px` per frame from the paint paths

The paint paths that actually draw the graph canvas write `app.graph_canvas_px` every frame, exactly like `component_rects`/`pane_rects` (renderer owns layout, publishes geometry):

- `paint_tiled` (src/gui/mod.rs:629), Graph slot branch: set `app.graph_canvas_px = Some((rect.width(), rect.height()))` before `paint_scene_in`.
- The quad-path graph pane (`paint_scene_in` at src/gui/mod.rs:613) sets it to that pane's rect.
- `GraphWindow::paint` (src/gui/mod.rs:725) sets it to the canvas size in points.

This revives `graph_canvas_center_world` (zoom anchor) and `graph_pan_if_overflow` (arrow-pan gate) with the real visible pane. The `seed_graph_camera` test helper at src/handler.rs:2598 can then publish the same way instead of using a bespoke `(960, 480)`.

Alternatives considered: computing the pane size in the handler from `pane_rects`. Rejected: `pane_rects` is cell-based, and the renderer is the established owner of layout geometry (ADR 4).

### D2: First-frame fit uses the real visible pane

`graph_window_fit_camera` gains a `viewport: (f32, f32)` parameter instead of the constant `WINDOW_FIT_VIEWPORT_W/H`. Its three call sites pass a real size:

- `main.rs` RedrawRequested: the window's inner size in points (`inner_size / scale_factor`) as a first-frame fallback.
- `paint_tiled`/quad path: the graph slot rect (already available as `rect`), seeding only when `graph_camera.is_none()`.
- `GraphWindow::paint`: the canvas size in points.

Because the loop is draw→read→dispatch, the tile's own rect is not known in `main.rs` before the first paint; the window-size seed covers frame 1, and from frame 2 the tile seed (with the slot rect) is exact. The fit stays dependency-filter aware: when `dependency_nodes` is non-empty the fit runs over the subset positions, preserving the selection logic currently at src/main.rs:217-225.

Alternatives considered: moving the whole seed into `main.rs` with the window size only. Rejected: the tile is a sub-rect, and centering accuracy was the reported defect. Keeping the seed in `main.rs` as the window-size fallback plus the paint-path tile seed gives an exact fit from the second frame with minimal churn.

### D3: Key assignments

In the graph-surface branch of `handle_event` (src/handler.rs:935-975):

- `Char('c')` (no modifier): `center_graph_camera()` — pan so the drawn bounds' center maps to the canvas center; zoom unchanged; no-op (with no status) before the first render or with no graph open.
- `Char('C')` (Shift+c, winit reports Shift+c as `Char('C')`): `fit_graph_camera()` — full refit against the published canvas size, preset reset to the fit index.
- Latency coloring moves to the `g c` prefix chord (the `g`-prefix branch at src/handler.rs:547-617 gains a `c` arm beside `v/g/d/o/w/s`).

The desktop-window path (`WindowGraphKey`, src/gui/mod.rs:78) gains `CenterGraph` and `FitGraph` variants mapped from `Key::C` / Shift+C in the window-keys extraction (src/gui/mod.rs:779-781) and dispatched in `handle_graph_window_frame` (src/handler.rs:2008). Latency coloring keeps no direct key on the desktop-window surface (it never had one there; the tile path's `g c` covers the app).

Status lines: `Graph centered` (or a neutral equivalent) for `c`, and the existing `Graph zoom NN%` style for `Shift+c` after the refit.

### D4: Zoom preset list

`GRAPH_ZOOM_PRESETS` becomes `[0.03125, 0.0625, 0.125, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0]` (9 entries). `0.03125` is exactly 2× below the previous minimum, satisfying the zoom-out requirement with the existing steps and wrap-around intact. The `1.0` entry (the fitted zoom) moves from index 5 to 6, so:

- a named constant carries the fit index (e.g. `GRAPH_ZOOM_FIT_INDEX: usize = 6`) with a doc note tying it to the `1.0` entry, replacing the hardcoded `5` at the reset sites (src/app.rs:2643, 3101) and the test assertions (src/handler.rs:2634, 2695);
- `fit_graph_camera` sets `graph_zoom_preset` to that constant so `Shift+c` and the first-frame fit agree with the preset cycle.

### D5: Help modal

`src/help.rs` updates the graph key table (row `c` becomes "center graph", new `Shift+c` "fit and center graph") and the g-prefix table (new `g c` "toggle latency coloring" beside `g o`).

## Risks / Trade-offs

- [First frame after opening the graph uses the window size, not the tile rect] → The tile publishes its exact rect the same frame, so from frame 2 (the first frame the user can press keys) fit/center/zoom are tile-accurate.
- [Preset list change shifts the fit index and any code/test that hardcodes preset indices] → Single named constant for the fit index; all reset sites and assertions route through it; tests re-assert the new wrap behavior.
- [`Char('C')` matching depends on the winit→`KeyEvent` conversion for Shift+c] → `from_winit_parts` already produces `Char('C')` for Shift+C; the handler tests drive both `Char('c')` and `Char('C')` through the shared dispatch to pin it.
- [Two writers (main.rs fallback and paint path) seed the camera] → Guarded by `graph_camera.is_none()` at both sites; the paint path's exact-rect seed wins in practice because it runs after the window-size fallback in the same frame cycle.

## Migration Plan

In-repo change, no config, no data, no dependency changes. Land as a normal feature-branch change; rollback is reverting the commit. No deployment steps.