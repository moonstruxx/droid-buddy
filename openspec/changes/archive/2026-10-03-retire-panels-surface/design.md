# Design

## Context

See `proposal.md` — Why. `module-ui-physical` made the physical view the module UI and removed `ViewType::Panels`, but left three artifacts of the retired surface:

- `src/gui/panels.rs` — `panels_spec`/`paint_panels`/`PanelsFrame`, `pub(super)`/`pub(crate)`, no longer reachable from `paint_panes`. The module is still declared (`mod panels;`) and its frame re-exported, so it compiles; clippy stays quiet only because `--all-targets` compiles the tests that call it.
- `src/handler.rs` — `handle_panels_frame(frame, app)` plus its tests, reachable only from tests.
- `src/help.rs` — `HelpView::Panels` (title `Panels / Physical`) and `HelpView::Physical` (title `Physical Rack`) describe the same surface with different tables; `active_view` reports `Panels` for the module UI.

The module UI's cell painting already lives in `src/gui/physical.rs` and shares `paint_cell`/`CellSpec` with the rest of the app, so the deleted module has no unique rendering logic.

## Goals / Non-Goals

**Goals:**

- Remove the retired Panels surface completely, so no type, module, or handler entry point survives without a consumer.
- Make the help modal describe one view per surface, named after the surface the user sees.

**Non-Goals:**

- Changing module-UI behavior, layout, or geometry.
- Changing `ViewType`, `PaneLayout`, or the physical/rack model.
- Keeping the deleted code behind a flag or feature gate.

## Decisions

**D1 — Delete the module rather than gate it.** `src/gui/panels.rs` has no production consumer; the module-UI path supersedes it. A `#[cfg]` gate or feature flag would preserve the dead surface and its tests, contradicting the repo's YAGNI anchor. *Alternative considered:* keep it as a "reference" renderer — rejected, because `src/gui/physical.rs` is the reference and the shared `paint_cell` already covers the cell anatomy.

**D2 — One help view per surface.** Replace `HelpView::Panels` and `HelpView::Physical` with a single `HelpView::ModuleUi` whose table merges the two: module-UI navigation/toggle/wheel/latch/label/shift/skeleton/zoom/pan rows plus the pane keys (`Tab`/`Shift+Tab`, `z`, `Alt+b`, `Alt+s`, `r`) and the global keys. *Alternative considered:* keep both views — rejected, because the modal would keep presenting two names for one surface, and `active_view` could only ever report one of them.

**D3 — Spec delta on `keybinding` only.** The `module-ui` spec's requirements describe rendering and interactions; none of them mention the help view or the deleted module, so deleting the module is not a spec-level change. *Alternative considered:* add a `module-ui` delta naming the help view — rejected as inventing a requirement to satisfy validation.

**D4 — Scope the delta to the enumeration, not the scenarios.** OpenSpec requires a MODIFIED block to repeat every scenario the current spec has — `openspec validate` rejects a delta that omits one, and `RENAMED` is honored only at requirement level (no archived change in this repo renames a scenario). The two scenarios this change would otherwise rewrite are named `Panels table lists split, …` and `Physical table lists zoom, …`, so rewriting their bodies to the module UI while keeping their names would leave each name contradicting its body. The delta therefore changes only the requirement's view enumeration and leaves the scenarios — and the pre-existing `\` left-pane-split drift they share — untouched. *Alternative considered:* fix the `\` drift here — rejected, because it forces exactly that name/body mismatch; it is recorded as a follow-up.

## Risks / Trade-offs

| Risk | Mitigation |
|---|---|
| Deleting 789 lines removes its unit tests | They cover a surface with no consumer; the module UI's own tests exercise the same shared cell painting and its geometry. The regression task asserts the surface is gone rather than leaving a stale test. |
| The help modal's view list changes (user-visible) | Covered by the `keybinding` delta and the help parity tests, which require every `HelpView` to have a non-empty table whose rows name keys the handler actually dispatches. |
| `handle_panels_frame` removal could break another caller | Verified reachable only from `handler.rs` tests; the change's regression task also asserts no `PanelsFrame`/`handle_panels_frame` remains. |
| The `keybinding` scenarios keep pre-retirement wording (`panels/physical view`, `physical rack view`) and the stale `\` split row | Out of scope by D4, recorded as a follow-up. The delta changes the view enumeration, so the contract's view list is correct even though the scenario names are historical. |
