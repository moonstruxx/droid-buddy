# Physical View Fader + LED Fidelity

## Why

The physical 1:1 view renders F-family controllers (P8S8 Faderbank, M4 Motorfader) as flat boxed `◉ %` cells, but the DROID hardware draws **vertical 20 mm (P8S8) / 60 mm (M4) sliders with an LED bar whose brightness tracks position**. Under the 1:1 live-mix requirement this is wrong: the user must be able to grab the same fader, at the same vertical position, and read its LED state directly off the screen. Three gaps block that: (1) no vertical track, so position is text-only (bead `droid_tui-1oq`); (2) mm→screen rounding makes adjacent cells share a screen column at non-default zooms, so published `component_rects` overlap and hover/click resolves by iteration order (bead `droid_tui-vj7`); (3) LED wiring is device-dependent (M4 fader buttons RGB + `L`/`R` registers, B32 white-only, master LEDs default-link to their CD channels) but the parser treats LED association as one convention (bead `droid_tui-iyq`-adjacent, memory #225). A fourth gap is dead code: the boxed-LED branch in `render_physical_cell` (gated `width>=5 && height>=3`) is unreachable at every zoom preset because no element cell reaches 5 cols (bead `droid_tui-mtf`).

## What Changes

- **Vertical fader track with LED bar**: F-family element cells render a vertical track glyph proportional to the component's value, with an amber LED bar whose fill maps to position. This replaces the flat `◉ %` boxed face for sliders/faders (P8S8 `F` family, M4 `F` family; resolved through the existing P→F register remap). Declared via the physical visual/geometry path so panel keeps its rendering.
- **Adjoining-cell rect clamping**: clamp adjacent mm→screen cell spans at draw time so `component_rects` never overlap at any zoom preset (75/100/150/200 %), while preserving the existing strict no-overlap regression at zoom 1.0 and extending it to all presets.
- **Device-dependent LED association**: `patch.rs` LED association recognizes per-device wiring — M4 fader touch plates are RGB (`L`+`R` registers per fader), B32 buttons are white-only, master LEDs default-link to their CD channels — while keeping a bare `led = L.N` and numbered `ledN = L.M` (suffix-paired with `buttonN`/`potN`/`encoderN`/`switchN`/`faderN`) authoritative. The explicit pairing remains the override; device defaults only apply when no explicit pairing exists.
- **Drop the unreachable boxed-LED branch**: remove the `width>=5 && height>=3` boxed-LED path from `render_physical_cell` (no element cell reaches 5 cols at any zoom), keeping the compact cell contract. Tests migrated in the 5.1 proof assert the compact contract.

## Capabilities

### New Capabilities
- `physical-fader-rendering`: rendering vertical fader tracks with position-driven LED bars for F-family controllers (P8S8, M4), and the draw-time clamping of adjacent element-cell rects at non-default zoom presets.

### Modified Capabilities
- `physical-scale-model`: element-cell rect clamping at non-default zoom presets (no overlapping `component_rects`), and removal of the unreachable `width>=5 && height>=3` boxed-LED branch from the physical cell path (the compact cell becomes the sole contract).
- `patch-parsing`: LED association becomes device-dependent (M4 RGB `L`/`R`, B32 white-only, master→CD default-link), with explicit `led`/`ledN` pairing still authoritative.
- `visual-validation`: the coverage matrix gains fader-column fixture rows across themes × widths (vertical track + amber LED bar faces), and the fader/led cell contract is pinned by regression tests.

## Impact

- **Code**: `src/ui.rs` (`physical_visuals`, `render_physical_cell`, `render_physical_full`), `src/physical.rs` (mm→screen cell span mapping / clamping), `src/patch.rs` (LED association), `src/regression.rs` (regression + visual snapshots), `src/theme.rs` (amber LED bar token).
- **Fixtures**: new fader-column and device-LED fixtures under `fixtures/`.
- **Docs**: `ARCHITECTURE.md` / `DESIGN.md` regenerated via `/make-*` after implementation.
- **No dependencies** added; no async/network/API change.
