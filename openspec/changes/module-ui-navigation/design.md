# Design

## Context

See `proposal.md` — Why. Four facts shape the approach.

1. The pane's world is already modelled: `physical_offset` + `physical_zoom` (`src/app.rs`) place the rack, `physical::rack_extent_points` gives its extent at any zoom, and `physical_overflow` already asks whether that extent exceeds the pane. A minimap needs exactly those numbers.
2. The graph's minimap is a working precedent for every part of this: `paint_minimap` (`src/gui/graph.rs`) draws panel, frames and viewport; `minimap_world_point` and `minimap_world_bounds` invert one published transform; `apply_minimap_press` and `release_minimap_drag` (`src/handler.rs`) own the press/drag/release path; `graph_minimap_panel_egui` is the published panel rect the pointer path checks first. The `minimap-region-zoom` change also established the "draw it even when the scene fits, with a clamped viewport box" rule.
3. The fit machinery already has the latch this change needs to re-arm: `physical_nav_latched` is set by `note_physical_navigation` and consulted by `apply_physical_fit_triggers`; `apply_physical_fit` clears it as a side effect of fitting.
4. `handle_graph_camera_key` binds `c` to centre and `Shift+C` to refit. On the module-UI pane both are free: the only `c` claims anywhere are the `g c` latency chord and `Ctrl+C` quit, both guarded.

## Goals / Non-Goals

**Goals**
- The pane can be navigated by place rather than by arrow-key scrolling: see where you are, jump there, and come back.
- One transform, one truth: the map, the pointer inversion and the overflow judgement read the same numbers.
- The two keys mean the same thing on both surfaces.

**Non-Goals**
- No new geometry source, no change to the fits or the zoom presets, no per-element detail in the map.
- No map state to persist; it is derived every frame from the offset, the zoom and the rack.

## Decisions

### D1 — The map maps the extent `physical_overflow` already computes

The world is the rack's case rect at the current zoom, taken from `physical::rack_extent_points`, the same call `physical_overflow` uses (`src/app.rs`). One isotropic scale maps it into the panel, so a rack that is wide and short maps wide and short, and the map's "you are here" and the arrow keys' overflow decision can never disagree about where the rack ends.

*Alternatives considered.* Mapping the pane instead of the rack would make the map a viewport-relative picture that changes shape as you pan — useless as a place-finder.

### D2 — One pure model, one transform, and its inverse

`physical::rack_minimap(case, modules, pane, offset, zoom) -> MiniMap { panel, modules: Vec<Rect>, viewport: Rect, transform }`, with the same transform tuple shape the graph publishes `(bx, by, bw, bh, ix, iy, sx, sy)`, plus an inverse (`minimap_rack_point`) so the handler can turn a pointer position into a rack point. Painter and pointer read one answer; neither re-derives a scale. This is the specific lesson `minimap-region-zoom` recorded after shipping an inline inverse in the handler.

The panel is 160 pt wide, its height `160 × rack_h / rack_w + 8` clamped to 56–120 pt, inset 8 pt from the pane's bottom-right: the multirow fixture's rack is one row of 457.2 × 128.5 mm — 3.56:1 — so its panel height lands on the 56 pt floor and its 17 modules get about 8.9 pt each; a 2.5:1 rack would take 72 pt. A fixed square panel would waste half its area and halve that module width.

### D3 — The panel claims the pointer before element hits

`paint_physical` publishes `physical_minimap_panel_egui`, and the pointer path checks it first — the same precedence `graph_minimap_panel_egui` has. This is the change's only real regression surface: the panel covers roughly two element cells in the bottom-right corner, so without the check a click there would focus an element under the map and a wheel would adjust its value.

### D4 — `c` and `Shift+C` mirror the graph's pair

`c` centres, `Shift+C` refits — the same two meanings the graph gives them, so the gesture does not have to be learned twice. On the module UI, `Shift+C` also clears `physical_nav_latched`, which is what makes the automatic triggers fit again after a manual pan; `f` clears it only as a side effect of an explicit fit, and nothing today says "keep fitting".

*Alternatives considered.* A persistent auto-fit mode (a latch of its own, a pane-title segment) buys the same outcome with a new state and a new segment to explain; the re-arm gives it in one line and keeps the title's vocabulary unchanged.

### D5 — Centring is a pure offset mutation

`c` needs no zoom change and no solver: resolve the focused token to its hardware index, read its element rect, and set `offset = pane_centre − rect_centre` at the current zoom. With nothing focused it centres the rack. Both are pure `App` mutations over `physical_pane_rect`, so they are unit-testable without a painter and they latch the view exactly as a pan does.

### D6 — No new token, no new title segment

The map draws in `accent` (panel border and viewport box), `muted` (case and module edges) and `focus_border` (the focused module), all of which exist in all three palettes. The pane title keeps its current segments — zoom, fit, labels, skeleton, pan — because the map is a navigation aid, not a state to report.

## Risks / Trade-offs

- **The panel eats two element cells in the bottom-right.** → D3's precedence makes the map a first-class surface rather than an overlay that swallows clicks; the existing hit-rect tests keep the element rects themselves unchanged, and a regression story pins that a press on the panel focuses nothing.
- **At 9 pt per module the map cannot name anything.** → By design: it shows place and extent, and the hover reveal plus the pane title still name the element. Non-goal, not a gap.
- **A drag on the map could be mistaken for a pane pan.** → The press is claimed by the panel before the pane's middle-drag path, and the marquee only starts on the map's own rect; a click (a zero-extent drag) pans to a point, so the two gestures never overlap.
- **`Shift+C` re-arming could fight a deliberate pan.** → It is the only key that re-arms, it is explicit, and it fits immediately, so the state is visible in the zoom the user lands on.
