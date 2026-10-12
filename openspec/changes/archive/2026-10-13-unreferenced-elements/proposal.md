# Proposal

## Why

The module UI violates its own spec on every partially-used controller: the
`module-ui` requirement "Unused elements are drawn" demands every hardware
element render dimmed and unlabelled, but `rack_geometry` emits cells only
for patch-declared components, so unreferenced elements never appear — most
visibly the E4/DB8E LED rings (a bare `[e4]` synthesizes 4 L components for
128 ring segments, so 124 segments are missing) and every unused button on a
partial B32. Bead `droid_tui-xbm` records the defect with evidence.

## What Changes

- **Unaddressed-cell emission.** `rack_geometry` emits every geometry-declared
  cell per module, not just cells addressed by patch components. Cells beyond
  `module.components.len()` flow through the already-built downstream path:
  `physical_spec` renders them dimmed and unlabelled with `component_index:
  None`, and `paint_panes` never publishes them to `component_rects` (already
  filtered). Skeleton coincidence holds automatically (same geometry source).
- **Pure enumeration in `physical.rs`.** A new
  `PhysicalModule::unaddressed_cells` (name TBD) walks the module's
  `cells` family map and returns cells whose `(kind_letter, element)` pair is
  not among the module's components (via the existing `token_element`), in
  deterministic order (family key, then row, then col — never `HashMap`
  order).
- **Fold-rule suppression for stray LEDs.** An unaddressed LED cell on a
  fold-rule controller (P2B8/B32/M4) is suppressed, never standalone —
  otherwise the fix would regress the button-led-composition fold. Rings,
  sliders, and jack-side LEDs render dimmed standalone. The spec recovers the
  token via the cell label for the suppression check.
- **No hit-testing change.** `component_rects` keeps its `Vec<(usize, Rect)>`
  shape; navigation, selection, label editor, and modifier latch operate on
  component indices and never see unaddressed cells.
- **Fixture, story, shots.** A partial-B32 + bare-E4 fixture with a regression
  story, and re-blessed screenshots (the rotary-stack shot gains its 124 dimmed
  ring segments).

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `module-ui`: "Unused elements are drawn" becomes implemented (emission +
  dimmed rendering + no-hit contract); "E4 rings follow the LED registers"
  gains the dimmed-unaddressed-segments scenario.

## Impact

- Touched: `src/physical.rs` (enumeration + tests), `src/gui/physical.rs`
  (emission + suppression + tests), `src/gui/mod.rs` (only if the
  republication test needs it — expected untouched), new
  `fixtures/unreferenced_elements.ini`, `src/regression.rs`,
  `tools/screenshots/**`, `docs/assets/*.png`.
- Behaviour the user can see: partial faceplates fill out with dimmed
  unlabelled elements; E4/DB8E rings render complete; full-front patches
  (bare P2B8/B32/M4) render byte-identical specs (no unaddressed cells).
- No `.ini` mutation, no config key, no persistence, no new dependency. One
  new pure method plus emission plumbing.
- Closes bead `droid_tui-xbm` on completion.

## Non-goals

- No S10 rotary/toggle glyphs (bead `droid_tui-a0f`), no P4B2/master/G8/X7
  face parity (bead `droid_tui-y57`), no shift colour (bead `droid_tui-cnb`).
- No geometry-data changes (`controller_geometry.json` untouched).
- No R-register chains, no dynamic colour.
- No hit-testing, navigation, or label-editor changes for unaddressed cells.
- No hand-edits to ARCHITECTURE.md or DESIGN.md (regenerate at archive).

## New types (YAGNI consumer check)

- `PhysicalModule::unaddressed_cells` (name TBD, returns addressed-excluded
  `ElementCell`s in deterministic order): consumed by `rack_geometry`
  (emission) and `physical_spec` (fold-suppression lookup). No new struct,
  field, variant, or derive — the `None`-index/`dimmed` spec path already
  exists.
