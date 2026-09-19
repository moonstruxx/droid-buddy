# Graph pane centering, fit, and deeper zoom-out

## Why

On the graph surface, `c` currently toggles latency coloring, which leaves no key for centering the graph, and the first-frame camera fit seeds against a hardcoded 1280×800 viewport (main.rs) instead of the actual visible pane, so centering and fitting in a narrow tile frame the wrong region. Zoom-out is also capped at 0.0625× the fitted zoom, which is less than the requested 2× zoom-out headroom.

## What Changes

- `c` on the graph surface centers the graph: it pans so the drawn content's center lands at the visible pane's center, keeping the current zoom. `Shift+c` refits and centers: it re-frames the whole graph against the visible pane. **BREAKING**: bare `c` no longer toggles latency coloring; that moves to the `g c` prefix chord.
- `g c` (g-prefix) toggles latency coloring, matching the other graph chords (`g g`, `g v`, `g d`, `g o`, `g s`).
- The camera fit, fit-and-center, and center operations use the real visible pane size instead of a hardcoded viewport: the paint paths publish `graph_canvas_px` per frame (the graph slot rect in the tiled layout, the window canvas size in the graph window), and the first-frame seed uses that size. This also revives the canvas-center zoom anchor and the arrow-pan-on-overflow gating, which are dead today because `graph_canvas_px` is only set in tests.
- `GRAPH_ZOOM_PRESETS` gains a `0.03125` step below `0.0625`, giving exactly 2× deeper zoom-out while preserving the existing steps and wrap-around. The preset index that means "fitted zoom" (1.0) moves from 5 to 6, and the reset sites follow.
- The help modal's graph key table and g-prefix table are updated to the new key assignments.
- Tests cover the key routing (`c` / `Shift+c` / `g c`), the center and fit camera math against real viewport sizes, the extended preset list and its wrap, and per-frame `graph_canvas_px` publication on both paint paths.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `signal-flow-graph`: the pan/zoom navigation and camera behavior requirements change (center key, fit-and-center key, viewport-accurate initial fit and centering, deeper zoom-out range).
- `keybinding`: the `c` key assignment changes (center on the graph surface, latency coloring moves to `g c`).

## Impact

- `src/app.rs`: `GRAPH_ZOOM_PRESETS` extended, `graph_canvas_px` published per frame, new `center_graph_camera` / `fit_graph_camera` helpers, preset reset index updated.
- `src/handler.rs`: graph-surface `c`/`Shift+c` handling, `g c` prefix branch.
- `src/main.rs`: first-frame camera seed uses the real window size instead of the 1280×800 constant.
- `src/gui/mod.rs` / `src/gui/graph.rs`: publish `graph_canvas_px` from the tile and window paint paths; viewport-aware fit helper.
- `src/help.rs`: key table rows.
- Tests in `handler.rs`, `app.rs`, `gui/graph.rs`.
- No dependency changes, no config changes, no changes to the layout solver or latency coloring semantics.

## Non-goals

- No mouse-gesture centering (middle-click or double-click to center).
- No per-surface camera persistence or camera state in config.
- No changes to the physical view's zoom or pan behavior.
- No new latency coloring semantics, only the key chord it lives on.