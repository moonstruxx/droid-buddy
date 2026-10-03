# Proposal

## Why

The `module-ui-physical` change retired the Panels *view* but left the Panels *surface* behind. `src/gui/panels.rs` (789 lines) is no longer dispatched by `paint_panes` — it still compiles only because its own tests call `panels_spec`/`paint_panels` — and `PanelsFrame`/`handle_panels_frame` are test-only. The help modal likewise carries two overlapping views for what is now one surface (`Panels` titled "Panels / Physical" and `Physical` titled "Physical Rack"). This violates the repo's YAGNI anchor ("every type, variant, derive, and trait has a current consumer or it gets deleted") and leaves the help modal naming a surface that no longer exists.

## What Changes

- Delete `src/gui/panels.rs` and its wiring (`mod panels;` and the `PanelsFrame` re-export in `src/gui/mod.rs`).
- Remove `PanelsFrame` from the `src/handler.rs` import and delete `handle_panels_frame` with its tests.
- Replace `HelpView::Panels` and `HelpView::Physical` with a single `HelpView::ModuleUi` (title `Module UI`) carrying one merged keybinding table; update `active_view` and the `paint_help` call site.
- Fix the `panels::paint_panels` reference in the `src/gui/physical.rs` doc comment.
- Correct the `keybinding` spec: the describable-view enumeration collapses `Panels/Physical` + `Physical Rack` into `Module UI`.
- Add a regression asserting no Panels surface remains.

No behavior change to the module UI itself, and no `ViewType` change — `ViewType::Panels` is already gone.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `keybinding`: the help modal's set of describable views changes — Panels/Physical and Physical Rack merge into a single Module UI view.

## Non-goals

- No change to module-UI rendering, layout, interactions, or geometry.
- No change to the physical/rack model or `RackLayout`.
- No change to `ViewType` or `PaneLayout` (already Panels-free).
- No new surface, flag, or configuration option.
- No change to the `module-ui` spec (its requirements never mentioned the help view or the deleted module).
- No fix for the `keybinding` requirement's pre-existing `\` left-pane-split drift. A MODIFIED block replaces the whole requirement and archive refuses dropped scenarios, so correcting it would mean rewriting a scenario whose name names the split — leaving the name contradicting its body. Recorded as a follow-up instead.

## Impact

**Affected code:**
- `src/gui/panels.rs` — deleted
- `src/gui/mod.rs` — module wiring removed
- `src/handler.rs` — import and `handle_panels_frame` removed
- `src/help.rs` — two views merged into one
- `src/gui/overlays.rs` — help call site
- `src/gui/physical.rs` — stale doc comment
- `src/regression.rs` — no-Panels-surface regression

**Affected spec:** `openspec/specs/keybinding/spec.md`

**No schema, dependency, data-format, or config changes.**
