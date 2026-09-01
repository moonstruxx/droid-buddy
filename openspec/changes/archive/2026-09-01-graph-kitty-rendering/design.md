# Graph kitty rendering — design

## Context

The graph surface (`src/ui.rs::render_graph`) renders the force-directed `graph_positions` (f32 world coords) into box-drawing characters with a single bounding-box fit. On large patches this collapses nodes to 1–2 chars wide and there is no pan/zoom (see `proposal.md` — Why). The repo is already kitty-only in intent and ships an unused `kitty-gfx` feature flag in `Cargo.toml`; the app is a layered monolith on ratatui 0.29 + crossterm 0.28 with a strict `TestBackend`/`insta` snapshot gate that must remain byte-identical.

## Goals / Non-Goals

**Goals:**

- Render the graph as an anti-aliased image via the kitty graphics protocol when supported: rounded-rect nodes, colored cable curves with arrows, rasterized text labels.
- Composite the image *beneath* the existing header/status/picker text (so text stays readable on top).
- Add pan/zoom with a legible initial camera fit while preserving every existing graph interaction (node drag, hover, `x` disable, `e` label overlay, diff/latency/topology-error coloring).
- Keep the `TestBackend` + `insta` snapshot path byte-identical (no perturbation from the image emit).
- Reuse the existing color-classification logic (`cable_color_with_diff`, latency ramp, error precedence) unchanged; only add a `Color → RGB` hop.

**Non-Goals:**

- No ratatui 0.30 / `ratatui-image` migration.
- No backend besides kitty (no sixel, iTerm2, or production ASCII fallback beyond kitty-unsupported/test path).
- No change to layout-solver constants, the graph model, parsing, validation, or diff.
- No image caching, animation, or incremental redraw.
- No pixel renderer for the panel/physical view.

## Decisions

### 1. Raw `\x1b_G` escapes, not `ratatui-image`

`ratatui-image` 11.x requires `ratatui ^0.30.1` (crates.io, unpacked today) — it cannot coexist with `ratatui = "0.29"`. The 0.29-compatible `7.0.0` is stale and drags in `image` + `chafa`/`pkg-config` + `icy_sixel` + optional `tokio`. Raw escapes (≈40 lines: base64 + zlib chunking) give exact `C=1` / `z=-1` placement control, stay on 0.29, and keep the `TestBackend`/snapshot path untouched because the emit is gated behind `#[cfg(feature = "kitty-gfx")]` while the `Buffer`-writing widget logic is identical.

- **Alternative considered:** `ratatui-image` → rejected (0.30 bump + heavy, network-shaped tree + opaque widget can't be placed under text with the required precision).

### 2. `tiny-skia 0.12` for the offscreen rasterizer

`tiny-skia` is the only candidate with first-class anti-aliased cubic/quadratic Bézier strokes (`PathBuilder` + `Path::stroke`) and smooth filled/stroked rounded rects, on a bounded `Pixmap::new(w,h) → Option<Pixmap>` (clamps runaway pan/zoom). `imageproc 0.27` assembles rects/paths geometrically (aliased edges), pulls the whole `image` crate + rayon, and has no first-class AA Bézier stroke.

- **Rounded rects:** `tiny-skia` has **no `RRect`** — build via `PathBuilder` with two `quad_to` segments per corner.
- **Text:** `tiny-skia` has **no text** — pair with `fontdue`.
- **Features:** `tiny-skia = { version = "0.12", default-features = false, features = ["std", "simd"] }` (we never PNG-encode; we send raw `f=32`).

### 3. `fontdue 0.9` for text labels + a bundled OFL font

`fontdue` (`font.rasterize(ch, px) -> (Metrics, Vec<u8> /* 8-bit coverage */)`) is dead-simple for mostly-ASCII circuit labels; composite each glyph's coverage over the opaque canvas (straight-alpha blend, keep alpha=255). Bundle the font with `include_bytes!("../assets/Hack-Regular.ttf")` — deterministic, headless-safe, no filesystem/TTY dependency.

- **Alternative:** `ab_glyph 0.2` (more battle-tested, fractional positioning) — acceptable fallback; `fontdb 0.24` only if system-font lookup is ever required (we don't need it).
- **Font choice:** Hack (SIL OFL) — monospace, matches the TUI aesthetic.

### 4. `base64 0.22` + `flate2 1` (zlib `o=z`), chunk ≤ 4096

The protocol payload is base64, compressed with zlib (RFC-1950, `o=z` — there is no `z` compression key), chunked to ≤4096 bytes. Only the first chunk carries the full control data (`a=t,i=N,f=32,s=W,v=H,o=z,m=1/0,q=2`), intermediate chunks carry only `m` (and `q`). Use `q=2` (suppress failures) on every command to keep ACK noise out of the single-threaded crossterm stdin.

### 5. `f=32` RGBA (premultiplied == straight) over `f=100` PNG

`Pixmap::data()` is premultiplied RGBA8. Paint an **opaque** background first so the whole canvas is alpha=255 (premultiplied == straight), then send `data()` as `f=32`. This avoids PNG re-encoding every pan/zoom step.

### 6. Capability detection: env + query handshake, cached

Two signals at startup: `KITTY_WINDOW_ID` env (fast, kitty + compatibles) plus the `\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\` / `\x1b[c` (DA1) handshake (terminals implementing the protocol without the env var). Cache the result as a `bool`. If unsupported → existing box-drawing path.

### 7. Camera model reuses the physical-view pattern

New `GraphCamera { zoom, pan }` maps world `f32` solver coords → pixels via a fit transform, guaranteeing a legible minimum node size on the initial fit. Inverse mapping (camera → pixel → cell) derives `graph_node_rects` so the existing `handle_graph_mouse` drag/hover apparatus works unchanged. Zoom via wheel / `+`-`-` presets, pan via arrows / wheel overflow — mirroring the physical-view `ScreenMapping` camera but at pixel resolution.

### 8. Placement / z-order / lifecycle

- Move the cursor to the graph area's top-left cell (`\x1b[{row};{col}H`, 1-based) before each emit.
- Transmit with `a=t` (no auto-place), then place with `a=p,i=N,c=COL,r=ROW,z=-1,C=1,q=2` (scale into the area's cell rect, `z=-1` under text, `C=1` don't move cursor).
- Keep the graph area's cells background-free so `z=-1` shows through; every ratatui text cell redraw lands at z=0, on top.
- Re-transmit with the **same `i=`** replaces the prior image (kitty deletes prior placements), so pan/zoom never exhausts image ids.
- On resize: re-derive `area`, re-render at the new `Pixmap` size, re-transmit + re-place (bounded `Pixmap::new` clamps runaway dimensions).
- Gate re-transmit to actual view/model changes (pan/zoom/filter/highlight), not every input tick.
- On exit / fallback to ASCII: `\x1b_Ga=d\x1b\\` to clear visible placements.

### 9. Colors via `Color → RGB`, no hardcoded RGB

Add a `Theme::rgb(Color) -> (u8,u8,u8)` hop (ANSI-16 → RGB) used only by the pixel rasterizer. The existing classification pipeline (error red > diff > latency ramp > cable kind; `cable_color_with_diff`, `circuit_color`, `graph_edge_latency_*`) is reused verbatim; the precedence runs unchanged, then each token's `Color` is converted to RGB.

### 10. Gating — rasterizer deps unconditional, emit gated

`tiny-skia`/`fontdue`/`base64`/`flate2` are unconditional dependencies so the rasterizer and camera compile and unit-test in the default suite; only the `\x1b_G` terminal-emit (`src/kitty_protocol.rs` transport) is gated behind `#[cfg(feature = "kitty-gfx")]`. This keeps `cargo test`/`cargo clippy` happy in both configurations and keeps the `TestBackend`/snapshot path byte-identical.

## Risks / Trade-offs

- **[Stray kitty ACK bytes pollute crossterm stdin]** → send `q=2`/`q=1` on every command; drain/ignore any `\x1b_G` response in `main.rs::run`'s event read, or use an image id / no placement id where an ACK is unwanted.
- **[Image cells with a `Style::bg` paint over the image]** → keep the graph area cells background-free (the empty region must not set a background behind the image).
- **[Premultiplied alpha banding on translucent AA edges]** → paint an opaque background first; classic AA blends against the background as intended.
- **[Font rendering differences between the bundled font and the user's kitty font]** → accept the bundled-font determinism (kitty-only, headless-safe); system-font lookup is a documented non-goal.
- **[Snapshots must not regress]** → the emit is fully behind `#[cfg(feature = "kitty-gfx")]`; default `cargo test`/`cargo insta test --check` stays byte-identical. The existing box-drawing graph tests are gated on `#[cfg(not(feature = "kitty-gfx"))]`.
- **[Kitty terminal not present in dev/CI]** → CI runs the box-drawing/snapshot path (no real terminal); the image path is exercised only by pixel-level unit tests over the rasterizer/camera and the raw emitter's string output.

## Migration Plan

No deployment/config migration. The change is additive and gated at runtime by capability detection: existing non-kitty terminals and the complete `TestBackend` suite behave exactly as before. The `Cargo.toml` feature `kitty-gfx` already exists (no-op); this change activates it. Rollback is a no-op — disabling the feature (or a non-kitty terminal) restores the box-drawing path with no code change.

## Open Questions

- **Bundled font chooser (Hack vs JetBrains Mono vs Fira Mono):** purely aesthetic; swap the single `include_bytes!` asset without touching specs/tasks. Defaulting to Hack.
- **Zoom cap / preset set:** mirror the physical view's `[0.75, 1.0, 1.5, 2.0]` presets for the graph, with a larger upper bound for the image path. Exact set is a tuning detail that can be fixed during implementation without changing the spec.
