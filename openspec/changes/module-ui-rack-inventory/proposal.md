# Proposal

## Why

The module UI is the app's only hardware surface, and it answers the wrong question:
*what does this patch use*, where a rack view has to answer *what is in my rack*.

Three gaps, all verified in the current code:

- **The master's own front is suppressed.** The master faceplate is always drawn
  (`physical.rs` pushes it at chain position 0), but `rack_geometry` emits its unused
  cells only when `module.controller != "CV I/O"` (`gui/physical.rs`). A patch that
  addresses `I1..I3` therefore shows three jacks on an 8 HP face that physically
  carries sixteen jacks, a 4×4 LED matrix and the SD button — the faceplate does not
  look like the hardware it depicts.
- **A module exists only if the patch mentions it.** Every other module is created
  from `patch.hw_components` alone, so a G8, an X7 or an R2C sitting in the rack never
  appears unless the loaded patch addresses it. The same rack renders differently per
  patch, and differently from the hardware.
- **No patch, no rack.** `physical_spec` returns `None` when `app.patch` is `None`, so
  the pane paints nothing at all until a file is loaded.

Two element-identity defects surface with them:

- **MASTER18's gate column can never render.** Its geometry labels are `Gin1`,
  `Gin2`, `Gout1..4`; `label_element` finds no number in them, so
  `unaddressed_cells()` skips the six cells (`element.is_none()`) and only `O1..O8` of
  the fourteen declared CV cells ever appear.
- **The same gap mis-resolves tokens.** `cell_for("I1")` on MASTER18 matches no cell
  with element 1 and falls back to the positional `cells.get(0)` — **O1**. MASTER18's
  `I1`/`I2` are logic inputs (blue-7 manual §2.4; the master-fit rules already treat
  `I1`/`I2` as gate-typed on that model), so a patch using them draws on the wrong
  jack. The R2M/R2C `serial in` / `serial out` cells are skipped for the same reason,
  which would make an expander faceplate empty.

## What Changes

- **A rack inventory in configuration.** `[[physical.rack.row]] modules = ["master",
  "P2B8 1", "E4 2", "G8 1", "X7 1"]` names the modules a row physically holds, in
  left-to-right order. Entries use the module-key vocabulary `assign` already uses
  (`"{controller} {instance}"`); `master` and `CV I/O` resolve to the master model the
  patch declares or the plan resolves, so one inventory serves both masters. An entry
  naming an unknown controller warns once and is skipped.
- **The chain becomes the inventory, with the patch distributed onto it.**
  `PhysicalLayout::build` takes `Option<&Patch>` plus the inventory: inventory modules
  first in declared order, patch-only modules appended, and a module the patch does
  not address carrying an empty component list. Without an inventory the chain is
  exactly today's.
- **The module UI paints a rack with or without a patch.**
  `physical_spec`, `App::physical_chain_and_rack` and both fits stop requiring a patch
  when an inventory exists; labels and states are empty and every cell is unaddressed.
- **Unaddressed emission follows the inventory.** The `CV I/O` suppression stays for
  patch-derived chains — existing users see byte-identical rendering — and is lifted
  for inventory-driven chains, so the master front shows all of its declared field and
  every inventoried module shows its full face.
- **Element identity becomes explicit in the geometry data.** `ElementCellEntry` gains
  optional `element`/`kind_letter` fields that `preprocess_cells` prefers over the
  label-derived values; MASTER18's gate cells are renamed to their real tokens (`I1`,
  `I2`, `G1..G4`) and the R2M/R2C serial jacks get explicit element numbers. That
  fixes both "never rendered" and the `I1`→`O1` mis-resolution.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `configuration`: a new requirement for the rack module inventory in
  `[physical.rack]` rows — the key vocabulary, the master alias, unknown-entry
  handling, and its precedence against `assign`.
- `physical-scale-model`: `Physical grid model` changes so the chain may come from a
  configured inventory (inventory order followed by patch-only modules) and may be
  built without a patch; `Rack model` changes so row membership comes from the
  inventory for listed modules; a new requirement covers explicit element identity in
  the embedded geometry data.
- `module-ui`: `Unused elements are drawn` changes so an inventory-driven chain renders
  the master's complete declared field (and the pane paints before any patch loads,
  with every cell unaddressed); `Master faceplate` changes so the faceplate shows the
  model's whole field, including MASTER18's gate column, once the chain is
  inventory-driven.

## Impact

- Touched: `src/physical.rs` (`RackRow.modules`, the module-key parser,
  `PhysicalLayout::build`, `RackLayout::pack`, `ElementCellEntry`, `preprocess_cells`),
  `src/config.rs` (sanitise + round-trip the new key), `src/app.rs` (chain, rack and
  fits without a patch), `src/gui/physical.rs` (`physical_spec`, `rack_geometry`
  suppression), `controller_geometry.json` + `tools/acquire_geometry.py`,
  `src/regression.rs`, `docs/config.md`, `docs/views.md`.
- Visible behaviour: with an inventory configured, the module UI shows the rack — every
  I/O face dimmed where the patch is silent — and it paints before a patch is loaded.
  Without an inventory nothing about the rendering changes.
- Unchanged: keybindings, theme tokens, patch parsing, validation and its findings, the
  signal-flow graph, the `.ini` writer, and hit-rect semantics for addressed elements.
- **BREAKING** (crate-internal only): `PhysicalLayout::build` gains parameters and
  takes `Option<&Patch>`; `gui::physical::physical_spec` may return `Some` with no
  patch loaded.

## Non-goals

- No hardware detection: the inventory is declared, never probed. A wrong inventory
  draws a wrong rack, and the app cannot tell.
- No interaction on unaddressed or inventoried-only cells: no hit rects, no hover
  reveal, no selection, no label editing, no modifier wash. They stay inert, exactly as
  unused elements do today.
- No ring presentation for the E4's 128 declared LED cells; they render as declared, so
  an inventoried E4 is a dense dim field. A one-circle-per-encoder presentation is a
  separate change.
- No `assign` migration: a module listed in a row is placed by that row, and `assign`
  keeps applying only to modules the inventory does not list.
- No new keybinding, no persistence beyond `config.toml`, no `.ini` writes, no change
  to the validation finding set.
- No hand-edits of `ARCHITECTURE.md` or `DESIGN.md`; both regenerate at archive.
