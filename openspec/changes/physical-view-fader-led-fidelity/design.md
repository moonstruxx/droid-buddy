# Physical View Fader + LED Fidelity — Design

## Context

See `proposal.md` Why. The physical 1:1 view is rendered from `src/physical.rs` (mm grid → screen mapping) into `src/ui.rs` (`render_physical_full` → `render_physical_cell`). F-family elements (P8S8 `F`, M4 `F`) are remapped from P registers via `physical::cell_for` (the P→F family remap at `physical.rs:589`). Fader cells today render through the generic `physical_visuals` (knob/encoder/fader → `◉ %`) and `render_physical_cell`, which still carries the panel-era boxed-LED branch gated on `width>=5 && height>=3` (unreachable — no geometry cell reaches 5 cols; per bead `droid_tui-mtf`). `component_rects` are published at `render_physical_full` (`ui.rs:817`) from the same mapped cell rect used for drawing.

## Goals / Non-Goals

**Goals**
- Render F-family faders as vertical tracks with a position-driven amber LED bar (replace the flat `◉ %` face).
- Make adjoined element-cell `component_rects` non-overlapping at every zoom preset (75/100/150/200 %).
- Make LED association device-dependent while keeping explicit `led`/`ledN` authoritative.
- Remove the unreachable boxed-LED branch.

**Non-Goals**
- No kitty-gfx/graphics protocol work (the escape hatch is noted only as a future option, not implemented).
- No change to the panel (non-physical) view's boxed-LED rendering, which is reachable and stays.
- No signal-flow-graph, validation, or other-view changes.
- No hardware bridge, persistence, or new dependencies.

## Decisions

### D1. Vertical track rendered in-cell via a fill glyph, not a new widget or graphics protocol

The physical cell is a small `Rect` (from a few to ~7 mm mapped to 1–2 cols × several rows). A dedicated glider/bar widget or kitty-gfx is overkill and out of scope. Instead `render_physical_cell` computes a horizontal-bar fill within the cell's interior, using a single glyph per cell row whose brightness/symbol encodes the fill fraction. The fader occupies a vertical strip (track) with the fill drawn bottom-up — the track's filled rows use a "lit" glyph and the remaining rows use a "dim" variant, so the proportion maps directly to value.

- Alternative considered: a `Block` per row (heavy, no value gradient), a kitty-gfx `Image` (out of scope, deferral flagged in Non-Goals), and a plain `◉ %` (status quo — rejected because it hides position).
- Why: the cell is too small for a widget and every nonzero width cell can represent position with a 2-state glyph per row, at zero new dependencies.

### D2. Fader visual lives in `physical_visuals` + `render_physical_cell`, not a separate render fn

`render_physical_cell` already branches per-kind via the `(symbol, state_text, color)` tuple from `physical_visuals`. We extend `physical_visuals` to return a `FaderVisual` marker (or a dedicated arm) for `ComponentKind::Fader` when the cell is F-family, and `render_physical_cell` specializes: when the `FaderTrack` marker is present, draw the vertical track + LED bar and skip the generic `◉ %` glyph. The `Fader` kind is distinguished from `Knob`/`Encoder` in `physical_visuals` (`ui.rs:853`), so this is a narrow branch.

- Why: keeps the geometry handoff (hit rect = rendered cell) intact and localized; the renderer-owns-layout invariant is preserved.

### D3. Amber LED bar uses a new theme token

Add a `fader_led_bar` token (amber) to `theme.rs` for classic/terminal/mono. The bar's fill fraction reuses the same bottom-up fill computation as the track, drawn in the amber token (distinct from the red `led` token, per DESIGN.md color semantics). Terminal maps it to `Reset`; mono maps it to a mid-gray so the bar remains distinguishable.

- Alternative: reuse `led` (red) — rejected: the physical LED bar is amber, and #222 + DESIGN.md demand the color match the hardware.

### D4. Draw-time clamping of adjoined rects, owned by the physical mapping

The root cause is `mm→screen` rounding at non-default zooms producing adjacent cells that share a column (`physical.rs` `ScreenMapping::mm_rect_to_screen`). Clamp at draw time in `render_physical_full`: as cells are emitted in stable order, track the previous cell's right edge and clamp the current cell's left to `prev_right`, so a shared column is owned by the earlier cell and the neighbor is shifted/clamped. Only the published `component_rects` need the clamp; the drawing can keep showing the geometric cell. This is deterministic and order-based, resolving the iteration-order nondeterminism in bead `droid_tui-vj7`.

- Alternative: change the rounding in `ScreenMapping` globally — rejected: it risks the 5.1 coincidence proof (skeleton cell == full cell), which must stay exact. Clamping only the hit rect leaves the drawn cells — and the skeleton coincidence — untouched.
- Why: minimal blast radius; the strict no-overlap assertion extends naturally to all presets (2.2).

### D5. Device-dependent LED association in `patch.rs`, default fallback only

`patch.rs` LED association already handles bare `led = L.N` and numbered `ledN = L.M` (suffix-paired with `buttonN`/`potN`/`encoderN`/`switchN`/`faderN`). Add a per-controller default table: M4 → touch plate's RGB `L`/`R` registers, B32 → white-only (no RGB), master → CD-channel default-link. The default is applied only when the section has no explicit `led`/`ledN` pairing; explicit pairing remains authoritative. This is data in `patch.rs` (or a small embedded table mirroring the geometry/controller approach), keyed by the resolved controller name, consulted at parse time.

- Alternative: infer LED from geometry like `geometry.rs` does — rejected: LED association is already a parse-time concern and the device table is small; mixing in the geometry module would couple parse to the rack geometry load.

### D6. Remove the boxed-LED branch, keep configurable minimum width as an explicit decision

`render_physical_cell`'s `width>=5 && height>=3` branch is dead in the physical view (no cell reaches 5 cols). Since the panel view's equivalent branch is reachable and stays, the physical fader work does not need it. Decision: remove the branch from the physical cell path entirely, accepting the compact cell as the sole physical contract (per spec `physical-cell-compact-only`). A minimum-cell-width reintroduction is explicitly deferred (Non-Goals) and would be a separate decision.

- Alternative: keep the branch and just widen fader cells — rejected: widening cells would break module-width fidelity (#222) and the skeleton coincidence proof.

## Risks / Trade-offs

- [Fader vertical track at cell height 1–2 rows is coarse] → Mitigation: the slot/fill glyph encodes the fraction with 2 states per row; at the M4 60 mm fader (tall cell) the resolution is adequate, and the spec pins 0/50/100 % snapshots so the face is unambiguous.
- [Clamping the hit rect diverges it from the drawn cell] → Mitigation: clamp only the shared-column boundary; the element still lands on its geometry cell, and the overlap-free assertion runs at every preset (2.2) to prove no real collision. The 5.1 coincidence proof remains exact because the skeleton-cell mapping is untouched.
- [Device-default LED table could mis-map an unusual patch] → Mitigation: defaults apply only when there is no explicit pairing; explicit `led`/`ledN` always wins; per-device association is pinned by fixtures (3.2).
- [Removing the boxed-LED branch could orphan a regression/snapshot] → Mitigation: the 5.1 compact-cell regression already asserts the compact contract; snapshots regenerate and any face change is reviewable via `insta`.

## Migration Plan

No data migration. Implementation is additive to the renderer/path and removes one dead branch. Spec deltas sync to `openspec/specs/` on archive; ARCHITECTURE.md / DESIGN.md regenerated afterward via `/make-*`.

## Open Questions

None blocking. (The kitty-gfx escape hatch remains a documented future option, not a decision.)
