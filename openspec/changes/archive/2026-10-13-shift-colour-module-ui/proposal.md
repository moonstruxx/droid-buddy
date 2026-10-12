# Proposal

## Why

Shift groups are invisible on the module UI — the app's only hardware
surface. `cell_visuals` already implements Button-kind shift tinting via the
theme's shift1–4 tokens, but every call site passes `is_shift_active =
false`, and the `CellSpec.shift_color` slot the painter consumes via
`unwrap_or` is always set to `None`. Pressing `1`–`4` changes panel borders
(legacy path) and the status bar, while the faceplate elements themselves —
including the composed button rings — show nothing. Bead `droid_tui-cnb`
records the gap and names the state ring as the natural place.

## What Changes

- **Wire the shift flag at the spec site.** `physical_spec` computes
  `is_shift_active = app.active_shift.is_some_and(|g| comp.shift_group ==
  Some(g))` per component and passes it to `cell_visuals` instead of
  `false`, activating the existing Button-kind shift arm (group token when
  the component belongs to the active group).
- **Populate `shift_color`.** When a component belongs to the active shift
  group, the spec sets `shift_color` to the active group's theme token
  (same match as `cell_visuals`); otherwise `None`. The painter's existing
  `shift_color.unwrap_or(color)` base and the composed ring outline pick it
  up with no painter change.
- **Membership-gated, dimmed-exempt.** Non-member elements render exactly as
  today; unaddressed (`None`-index) cells never receive shift colour; with
  no shift active every spec is byte-identical to today.
- **Boundary kept:** non-Button kinds keep their rendering (`cell_visuals`
  shift arm covers Button only); labels keep text colour (the ring/glyph
  carries the signal).

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `shift-visualization`: ADD a module-UI element requirement (active-group
  members tint ring/glyph in the group token; non-members unchanged; no
  shift is a no-op).

## Impact

- Touched: `src/gui/physical.rs` (spec wiring + tests), possibly
  `src/regression.rs` (story), `tools/screenshots/**` + `docs/assets/*.png`
  (only if the harness can seed shift state — else byte-identical re-bless).
- Behaviour the user can see: pressing `1`–`4` tints member button faces on
  the module UI in the group colour; everything else unchanged.
- No `.ini` mutation, no config key, no persistence, no new dependency, no
  keybinding change. No new type, field, variant, or derive (both slots
  already exist).
- Closes bead `droid_tui-cnb` on completion.

## Non-goals

- No non-Button-kind shift tinting (knobs/switches keep current rendering).
- No label colour change, no border/marker additions on the module UI.
- No source-viewer or graph shift changes.
- No shift behaviour change (activation/clear/status bar untouched).
- No hand-edits to ARCHITECTURE.md or DESIGN.md (regenerate at archive).

## New types (YAGNI consumer check)

- None. `CellSpec.shift_color` and the `cell_visuals` shift arm already
  exist with their consumers; this change only feeds them.
