# Proposal

## Why

The module UI's composed button faces still lie about colour: every folded LED
fill renders white on RGB devices even when the patch sets a colour, because
nothing ever constructs `ComponentState::Led { value, rgb }` — the parser
creates every LED `Off` and the UI toggle only flips `Off`/`On`. Bead
`droid_tui-2n4` records the defect against the `module-ui` requirement "LED
colour and brightness come from the patch", which already requires patch-set
colour on RGB controllers.

## What Changes

- **Static LED colour/brightness resolution at load.** For each LED bound to an
  element, resolve its colour source from the owning circuit section's
  `ledcolor*` param (same-suffix pairing first, else the section's single
  `ledcolor` for single-LED circuits) and its brightness from `ledvalue*` the
  same way. Values resolve statically: float literals directly, constant
  expressions through the existing `expression.rs` evaluator, single-hop cable
  refs through the cable index when the producer output is itself constant.
  Anything dynamic resolves to `None` and keeps today's white/floor rendering.
- **Construct `Led { value, rgb }`.** An LED with a resolved colour or brightness
  source loads as `Led { value, rgb }` instead of `Off`; LEDs with no source keep
  `Off`, so every existing default-state test keeps passing. B32 stays
  brightness-only through the existing `led_capability_for_controller`
  (`White`), so a resolved colour is dropped there by design.
- **Toggle preserves colour.** `toggle_component` keeps `rgb` across flips
  (`Led { v, rgb }` alternates `value` 0.0/1.0 instead of collapsing to plain
  `On`), so the `Ctrl+Enter` chord and `Enter`/`Space` never bleach a coloured
  LED. Plain `Off`/`On` LEDs keep the old behaviour, so the committed chord
  tests stay green.
- **Scalar→RGB table.** The DROID `ledcolor` value is a single scalar (corpus:
  literals `0`–`1.2`, constant expressions, cable refs — e.g.
  `droid_mpfs5melody2.ini:456-476`). It maps through a documented hue table
  derived from the observed corpus range. The R-register channel table from the
  manual could not be verified (dead manual URLs, unowned RAGFlow dataset), so
  the table is pinned as our mapping with hardware verification filed as a
  follow-up — not as manual truth.
- **Fixture, story, shot.** A new fixture with literal-coloured LEDs (one lit at
  load via `ledvalue`) plus a regression story and a re-blessed screenshot.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `module-ui`: "LED colour and brightness come from the patch" gains static
  resolution (source matching, const-folding, fallback) and the toggle-
  preservation scenario; the white-only B32 scenario is unchanged.

## Impact

- Touched: `src/patch.rs` (resolver + load wiring + hue table + unit tests),
  `src/handler.rs` (toggle preservation + tests), `src/gui/physical.rs`
  (painter test only — it already reads `led_rgb`), new
  `fixtures/led_register_colour.ini`, `src/regression.rs`,
  `tools/screenshots/**`, `docs/assets/*.png`.
- Behaviour the user can see: LEDs with static patch colours render in colour
  (lit ones immediately, dark ones as a coloured floor core); everything else
  renders exactly as today.
- No `.ini` mutation, no config key, no persistence, no new dependency. One new
  pure resolver plus one pure hue function.
- Closes bead `droid_tui-2n4` on completion; files a hardware-verification
  follow-up for the hue table.

## Non-goals

- No R-register colour chains (`ledcolor = R17` with `R17` written elsewhere):
  R17+ are CV/gate outputs in the observed corpus, and the master-LED
  R-channel convention is unverified. Such refs fall back to white; wiring them
  through is a separate change.
- No dynamic colour (cables driven by live circuit outputs resolve to `None`).
- No E4-ring per-segment colours beyond what the fold already does (rings keep
  rendering from their segment states; coloured segments appear wherever the
  segment state carries `rgb`).
- No `buttoncolor` params (observed once in the generator; same mechanism could
  cover it later).
- No change to B32 white-only behaviour, no shift colour, no geometry changes.
- No hand-edits to ARCHITECTURE.md or DESIGN.md (regenerate at archive).

## New types (YAGNI consumer check)

- `Patch::resolve_led_colour` (name TBD by implementer, free function beside
  `folded_led_for_element`): consumed by the patch loader for LED initial
  state. Returns the resolved `(value, rgb)` or `None`.
- `led_colour_for_value(f64) -> [u8; 3]` (name TBD): pure hue mapping,
  consumed by the resolver only.
- No new `ComponentState` variant (`Led` already exists), no new field, no new
  derive.
