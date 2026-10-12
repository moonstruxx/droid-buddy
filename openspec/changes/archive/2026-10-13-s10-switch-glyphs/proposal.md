# Proposal

## Why

The S10 faceplate lies about two of its controls: its two 8-position rotary
switches render with the same `▣`/`□` toggle glyph as its eight 3-position
toggles, although the `module-ui` spec already requires a distinct rotary
glyph. The geometry data already annotates the difference (`"kind":
"rotary"` on S1.1/S1.2, `"small"` on S1.3–S1.10), but `ElementCellEntry` has
no `kind` field so serde drops the annotation at load, and `cell_visuals`
emits one glyph set per `ComponentKind::Switch`. Bead `droid_tui-a0f`
records the defect.

## What Changes

- **Carry the kind annotation.** `ElementCellEntry` gains `kind:
  Option<String>` (serde default; absent stays `None`), `ElementCell` gains
  the same field, and `preprocess_cells` carries it through. Unknown kinds
  and old data without annotations behave exactly as today.
- **Branch the Switch arm.** `cell_visuals` renders rotary-kind switches
  with a round glyph set (`◉`/`○`, matching the panel's round controls)
  versus the toggle set (`▣`/`□`) for everything else. State text
  (ON/OFF/percentage) is unchanged.
- **Geometry-side only.** The annotation stays faceplate data on the
  geometry cell; `HwComponent` gains no field. M4 `touch` annotations ride
  the new field untouched (the fold rule already covers touch plates).
- **Fixture, story, shots.** An S10 fixture with a regression story and
  re-blessed screenshots (S10 faces change glyphs).

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `module-ui`: "Faceplate element fidelity" gains the kind-annotation
  mechanism for the existing S10 rotary-vs-toggle scenario (previously
  specified but unimplemented).

## Impact

- Touched: `src/physical.rs` (fields + carry + tests),
  `src/gui/physical.rs` (Switch-arm branch + tests), new
  `fixtures/s10_switch_kinds.ini`, `src/regression.rs`,
  `tools/screenshots/**`, `docs/assets/*.png`.
- Behaviour the user can see: S10 rotary switches render round, toggles
  square; every other controller renders exactly as today (no annotations
  consumed yet).
- No `.ini` mutation, no config key, no persistence, no new dependency, no
  keybinding change. No new struct/variant/derive — two `Option<String>`
  fields on existing structs.
- Closes bead `droid_tui-a0f` on completion.

## Non-goals

- No M4 touch rendering change (fold rule covers it; the annotation is
  preserved for future use).
- No other controller consumes `kind` yet (E4/DB8E rings, P8S8 sliders keep
  current rendering).
- No state-text or interaction change (toggles still toggle the same way).
- No geometry-data changes (`controller_geometry.json` untouched).
- No hand-edits to ARCHITECTURE.md or DESIGN.md (regenerate at archive).

## New types (YAGNI consumer check)

- None. Two `Option<String>` fields (`ElementCellEntry.kind`,
  `ElementCell.kind`) consumed by `preprocess_cells` and the Switch arm.
