# Graph kitty rendering

## Why

The signal-flow graph surface currently renders the force-directed layout as ASCII box drawing. On real patches the solver's spread is squeezed into a single bounding-box fit, so nodes collapse to 1–2 characters wide, edges become unreadable, and there is no pan or zoom. The graph is effectively unusable at real patch scale, which defeats its purpose.

## What Changes

- Render the graph surface via the **kitty graphics protocol** when the terminal supports it: anti-aliased rounded-rect circuit nodes, colored cable curves with directional arrows, and rasterized text labels, replacing the box-drawing presentation.
- Add **pan/zoom navigation** (wheel + arrows) on the graph surface with a legible initial camera fit.
- Introduce an offscreen camera + rasterizer + kitty-emitter stack gated behind a `kitty-gfx` cargo feature **and** runtime kitty capability detection. **BREAKING** presentation change: in a kitty terminal the graph surface's default output becomes the image renderer; the box-drawing renderer is retained only for unsupported terminals and the `TestBackend`/snapshot suites.
- No ratatui 0.30 bump (raw `\x1b_G` escapes, not `ratatui-image`). All existing graph interactions are preserved on the image path: node drag → re-settle, hover highlight, `x` per-circuit disable, `e` label overlay, diff coloring, latency ramp, and topology-error highlight.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `signal-flow-graph`: the graph surface may render via the kitty graphics protocol when supported — anti-aliased rounded-rect nodes, colored cable curves with arrows, rasterized labels, image composited under the header/status/picker text layer; supports pan/zoom navigation with a legible initial fit; degrades to the existing box-drawing renderer off the kitty path. Existing interactions (node drag, hover, `x` disable, `e` label overlay, diff/topology-error/latency coloring, topology-error edge red) are preserved.

## Impact

- **Code**: new `src/graph_render.rs` (camera + offscreen rasterizer), new `src/kitty_protocol.rs` (emitter + capability detection); `src/ui.rs::render_graph` dispatch; `src/theme.rs` (`Color`→RGB for the pixel path); `src/app.rs` (`graph_camera` state); `src/handler.rs` (`handle_graph_mouse` wheel/arrow zoom+pan).
- **Dependencies**: `tiny-skia 0.12`, `fontdue 0.9`, `base64 0.22`, `flate2 1`; one bundled OFL-licensed monospace font (Hack) via `include_bytes!`.
- **Schema / behavior**: parsing, validation, and diff are untouched; the graph model (`src/graph.rs`) and layout solver (`src/layout.rs`) are unchanged.

## Non-goals

- No ratatui 0.30 / `ratatui-image` migration (raw `\x1b_G` is intentionally chosen).
- No backend besides kitty (no sixel, no iTerm2, no production ASCII fallback beyond the kitty-unsupported/test path).
- No change to the force-directed layout constants or the graph model (rendering-only); layout-sparsity tuning is future work.
- No image caching, no animation, no incremental redraw beyond a static per-frame image.
- No second pixel renderer for the panel or physical view.
