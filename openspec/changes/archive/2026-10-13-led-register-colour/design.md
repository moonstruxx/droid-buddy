# Design

## Context

See `proposal.md` - Why for the motivation. The current state shaping the
approach: `ComponentState::Led { value, rgb }` exists (`src/patch.rs:255`)
with the doc "for devices that support it", but the only constructor site in
spirit is absent — the parser defaults LEDs to `Off` (`src/patch.rs:2838`)
and `toggle_component` (`src/handler.rs:2437`) maps any non-`On` state to
plain `On`, destroying any `rgb`. The painter already consumes the resolved
form: `physical_spec` reads `ComponentState::Led { value: _, rgb }` into
`CellSpec.led_state`/`led_rgb` (`src/gui/physical.rs:340-348`) and
`paint_composed` renders `rgb` or falls back to white (861-868, 961-968).
So the entire gap is *producing* the value, not consuming it.

Corpus grounding (`ext/droid-lsp/droid_meta_mpfs/droid_mpfs5melody2.ini:440-500`,
`motokit.ini`, `test_mpfs1.py`): `ledcolor`/`ledvalue` are single-scalar
params taking float literals (`0`, `0.2` … `1.2`), constant expressions
(`_INT_CLOCK_UNMUTED * -0.4 + 0.8`), and cable refs (`_COLOR_DEGREE_FADER`,
produced by a `multicompare` mapping scale degrees to colour scalars).
`motorfader` carries one `ledcolor`/`ledvalue` pair for its single fader LED;
`fadermatrix` carries `ledcolor1..4`/`ledvalueXY` per column.

## Goals / Non-Goals

**Goals:**

- LEDs with statically-known patch colours render in colour at load.
- Colour survives toggles.
- Every unresolvable case renders exactly as today (white/floor), so the
  change is strictly additive in behaviour.

**Non-Goals:** everything in the proposal's Non-goals.

## Decisions

### D1 — Resolve colour/brightness from the owning section's params

For each LED component, look at the circuit sections that reference its token:
a `ledcolorN`/`ledvalueN` whose numeric suffix matches the element entry's
suffix wins (the same pairing convention as `ledN`/`buttonN`); otherwise the
section's single `ledcolor`/`ledvalue` applies when the section drives one LED
(`motorfader`, `faderbank`); `fadermatrix` columns match by element suffix.

*Why:* it reuses the parser's own pairing convention instead of inventing a
second one, and it matches every observed corpus shape.

*Alternatives:* resolve through the cable index from the LED token (LEDs are
sinks, not sources — nothing to follow); evaluate at paint time per frame
(pure waste; the patch is static).

### D2 — Static values only, through the existing evaluator

A param value resolves when it is a float literal, a constant expression
foldable by `expression::evaluate_droid_expr` with an empty map (plus the
`1V`/`V` suffix convention it already implements), or a single-hop `_CABLE`
ref whose producer `output*` param resolves the same way. Anything else —
dynamic cable, unknown token, div-by-zero — yields `None`.

*Why:* the evaluator already encodes DROID expression semantics
(left-to-right, no precedence); reusing it keeps one semantics source. The
single-hop cap bounds the walk (no fixpoint, no cycle risk beyond the
cycle-safe structures underneath).

*Alternatives:* multi-hop fixpoint resolution (over-engineering for literals;
the corpus's dynamic cases would still fall back).

### D3 — Scalar→RGB as a documented hue table, not manual truth

`led_colour_for_value` maps the scalar through a hue wheel covering the
observed corpus range (`0`–`1.2`, clamping outside). The mapping is pinned by
unit tests as *our* table, with a code comment citing the corpus evidence and
the unverifiable manual R-channel table.

*Why:* the manual page 404s and the RAGFlow `droid` dataset is unowned by
this key, so no authoritative table is obtainable right now; a principled,
tested, clearly-attributed table fixes the reported defect (always white)
while hardware verification rides a follow-up bead.

*Alternatives:* block the change on manual access (stalls a fixable bug on an
externality); hardcode per-controller RGB guesses from fixture aesthetics
(less principled than the corpus range).

### D4 — Only upgrade state when a source resolves

The loader sets `Led { value, rgb }` solely for LEDs with a resolved colour
or brightness source; all others keep `Off`. `value` comes from `ledvalue*`
(default 0.0 floor behaviour preserved through the painter's existing floor),
`rgb` from D3 unless `led_capability_for_controller` is `White` (B32 drops
the colour and keeps brightness).

*Why:* every committed default-state test (`device_led_defaults`,
`folded_led_none_when_no_binding`, the chord tests on uncoloured fixtures)
keeps passing untouched; the change is invisible without colour sources.

### D5 — Toggle preserves `rgb`

`toggle_component` on `Led { v, rgb }` flips to `Led { 1.0 - v.min(1.0).max(0.0) > 0.5 ? 0.0 : 1.0, rgb }` —
i.e. value alternates 0.0/1.0, `rgb` carried through. Plain `Off`/`On` LEDs
keep the exact old arms.

*Why:* otherwise the first `Ctrl+Enter` on a coloured LED bleaches it to
white-on — the feature would self-destruct on contact. Keeping the old arms
for non-`Led` states holds the committed chord tests green.

## Risks / Trade-offs

- [The hue table may differ from hardware] → attributed as ours, pinned by
  tests for self-consistency, hardware-verification bead filed at close.
- [`ledvalue` semantics assumed as brightness 0..1] → corpus shows `ledvalue
  = 1` alongside colours and `-0.3` for dim; value clamps to [0,1]; painter
  floor preserved.
- [Same-suffix pairing may misattribute on exotic circuits] → explicit
  single-param fallback covers the observed circuits; misattribution renders a
  wrong colour, never a crash (pure function, `Option`).
- [R-register colour writes stay white] → documented Non-goal with the corpus
  reason (R17+ are CV/gate outputs); no silent wrongness, just the old
  fallback.

## Open Questions

- None that change what's built. The hue table's hardware fidelity is filed
  for verification, not left open.
