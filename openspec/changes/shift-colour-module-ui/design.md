# Design

## Context

See `proposal.md` - Why. The current state: `cell_visuals(comp,
is_shift_active, fader)` (`src/gui/physical.rs:1166`) matches Button-kind
components against `comp.shift_group` for the shift1–4 tokens, but all three
call sites (345: the spec, 643: a label helper, 1767: tests) pass `false`.
`CellSpec.shift_color: Option<Color>` (88) is consumed at paint as
`shift_color.unwrap_or(cell.color)` (854) but set to `None` unconditionally
(441). `app.active_shift: Option<ShiftGroup>` and
`HwComponent.shift_group: Option<ShiftGroup>` are both live on the same
objects the spec already holds. The composed ring outline derives from the
spec `color`/base, so feeding the slot tints glyph and ring cells alike.

## Goals / Non-Goals

**Goals:**

- Active-group members tint on the module UI; everything else pixel-identical.
- Zero painter change (consume existing slots only).

**Non-Goals:** everything in the proposal's Non-goals.

## Decisions

### D1 — Compute membership at the spec site, reuse the existing arm

`let is_shift_active = app.active_shift.is_some_and(|g|
comp.shift_group == Some(g));` passed as the flag at line 345 (and the 643
helper where its component is in scope — check whether that path needs it;
if it serves labels detached from shift context, leave it `false` with a
comment).

*Why:* one predicate at the data site; the theme-token match inside
`cell_visuals` stays the single source for group→token.

*Alternatives:* resolve the token in the spec and bypass the arm (two
sources for the same mapping).

### D2 — Populate `shift_color` from the same membership

When `is_shift_active`, set `shift_color` to the group token (mirror the
`cell_visuals` match: shift1–4); else `None`. Unaddressed cells (`ci >=
len`) always `None`.

*Why:* the composed face ignores `cell_visuals` colour (it draws its own
outline from the base) — without the slot, rings would stay untinted and
only glyph buttons would change, splitting the surface into two shift
languages. The painter already consumes the slot.

### D3 — No-shift means byte-identical

With `active_shift == None`, flag is `false` and slot is `None` for every
cell — the exact current values. Existing headless tests (which never set a
shift) stay green untouched; coincidence and publication proofs unaffected.

### D4 — `cell_visuals` keeps its shape

No signature change; non-Button arms ignore the flag as today. The 1767
test call site keeps `false` (unit scope).

## Risks / Trade-offs

- [Ring + glyph could double-signal on composed faces] → intended: the ring
  IS the button's state channel there; tinting it is the bead's ask, and the
  glyph row is absent on composed cells so no doubling occurs.
- [Shift + modifier-wash overlap] → wash and shift coexist by design
  (orthogonal channels); the ring tint follows the same precedence the base
  colour already obeys — painter order unchanged, so no new conflict.
- [Screenshot harness paints one unshifted frame] → specs without shift are
  byte-identical, so the re-bless is a no-op check unless the harness can
  seed `active_shift` (task 3.2 decides; a seeded shift shot is preferred if
  reliable).

## Open Questions

- None that change what's built.
