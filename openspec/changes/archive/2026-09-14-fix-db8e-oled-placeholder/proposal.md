## Why

Task 2.1 of native-egui-rendering ported `ui.rs::render_physical_full/skeleton` to `src/gui/physical.rs`, but the DB8E OLED upper-band placeholder (ui.rs::render_rack_structure lines ~1032-1097, driven by `physical::db8e_display_state_for_layout`) was not carried over. The db8e module faceplate renders without its display band.

## What Changes

- Add OLED band to `paint_physical` in `src/gui/physical.rs`, deriving B-grid top from `module.cells['B']` and state text via `physical::db8e_display_state_for_layout`.
- Extend paint shape/label test to cover a db8e module.

## Capabilities

### New Capabilities
- none

### Modified Capabilities
- none — restoration of prior UI parity

## Impact

- `src/gui/physical.rs` paint_physical
- Tests in `src/gui/physical.rs`

## Non-goals

- No change to `physical.rs` logic beyond using existing `db8e_display_state_for_layout`
