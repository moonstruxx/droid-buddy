# Proposal

## Why

The module UI's button faces do not read as buttons, and the specs already say
they should. On a P2B8 or B32 each button paints a text glyph (`○`/`●`) with a
clipped label, its LED paints a second small circle **as its own cell** in the
middle of the same area, and a third copy paints as a tiny square in the cell's
top-right corner. On M4 touch plates the LED cell is not even inside the plate — it
sits below it.

The composed rendering is the documented contract: `module-ui` requires that an LED
belonging to an element "SHALL render as part of that element and SHALL NOT render
as a standalone cell", `physical-scale-model` repeats it for the compact cell, and
DESIGN.md states "LEDs are never drawn as standalone cells". As built the
suppression never happens: `physical_spec` builds a `folded_leds` set and never
reads it, while `rack_geometry` emits a cell — and therefore a hit rect — for every
component whose token resolves in the geometry, LED tokens included.

Two measurements underpin the fix:

- **The physical mapping is anisotropic.** mm→screen uses 0.15 cells/mm
  horizontally but 0.3 cells/mm vertically, and `rack_geometry` then scales both
  axes by a single points-per-cell factor. Net 1.5 pt/mm horizontally against
  3.0 pt/mm vertically — verified against the shipped `docs/assets/module-ui.png`
  (a 20 mm grid step measures 60 px across; P2B8 button rows 15 mm apart measure
  90 px down). The plate is drawn twice as tall as its real proportion, every
  element cell is twice as tall as it needs to be, and a physically round button
  maps to a 2:1 ellipse. "Draw the LED inside the button's circle" cannot be honest
  until the mapping is isotropic.
- **The LED's own cell is the only thing that has ever moved an LED.** The parser
  creates every LED component `Off` and nothing else writes one; removing the cell
  without a replacement would freeze the new fill at its floor forever.

## What Changes

- **Isotropic physical mapping.** One cells-per-mm factor for both axes, so a square
  panel cell maps to a square point rect and a round button maps to a circle. The
  plate's drawn proportions become true (a P2B8's 25.4 × 128.5 mm draws at
  1 : 5.06 instead of 1 : 10.1), element cells stop being half empty, and the
  rack's vertical overflow halves. The width-only fit contract is unchanged.
- **Composed button face.** On the controllers whose LED lights the button face —
  P2B8 and B32 round, M4 touch plate rectangular — a button-kind cell renders as a
  shape rather than a glyph: an outline whose colour carries the button's on/off
  state, with the bound LED as an inner fill (a concentric disc, or an inner
  rectangle on the M4 plate) whose extent and colour follow the LED level. The label
  renders under the shape when the cell has room; the state-text row goes away
  because the shape carries the state. Buttons on every other controller keep the
  glyph rendering.
- **The folded LED's own cell is suppressed.** `physical_spec` finally consumes its
  fold set for those controllers: the LED cell is neither painted nor published to
  `component_rects`. The skeleton and the full render both drop it, so the
  coincidence contract holds.
- **One binding source.** A pure `folded_led_for_element` resolver — the patch's
  explicit `led` pairing first, otherwise the controller's positional default —
  replaces the private duplication inside `physical_spec` and feeds the interaction
  layer, so painter and handler cannot disagree about which LED belongs to an
  element.
- **The LED stays reachable: a secondary handle.** `Ctrl+Enter` on the focused
  element toggles its folded LED and reports it; `Enter`/`Space` keep toggling the
  element itself. `j`/`k` and clicks no longer land on LEDs. Help lists the chord.
- **Visual artifacts and tests.** A new fixture covering P2B8 + B32 + M4 with
  buttons and LEDs, regression tests for the isotropic mapping, the composed face,
  the suppression and the chord, and re-rendered `docs/assets/*.png`.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `module-ui`: button faces render as composed state shapes instead of glyphs; the
  folded LED renders inside its element's face and publishes no cell; LED colour
  and brightness drive the fill; the interaction set gains the LED toggle chord.
- `physical-scale-model`: the 1:1 view maps millimetres to screen with one isotropic
  scale instead of aspect-compensated per-axis factors; the compact-cell contract
  gains the composed-face rule in place of the state-text row; button state renders
  as the composed shape.
- `fader-led-contrast`: the LED-indicator requirement covers the folded inner fill
  as well as standalone LED cells, with the same brightness floor.
- `keybinding`: the module UI binds `Ctrl+Enter` to toggle the focused element's
  folded LED, and help documents it.

## Impact

- Touched: `src/physical.rs` (mapping constants and `ScreenMapping`),
  `src/gui/physical.rs` (fold rule, suppression, composed painting, `CellSpec`, the
  published-rect tests), `src/gui/mod.rs` (the published-rect test),
  `src/patch.rs` (the folded-LED resolver), `src/handler.rs` (the chord),
  `src/help.rs` (table row and the key-set assertion), a new
  `fixtures/button_led_composition.ini`, `src/regression.rs`, `tools/screenshots/**`,
  `docs/assets/*.png`.
- Behaviour the user can see: the plate's proportions change (the isotropic fix),
  buttons become shapes, LEDs stop appearing as separate dots, and the LED chord is
  new. Everything else on the module UI — faders, switches, knobs, encoders, CV I/O,
  the DB8E band, the performance view (it reuses the same cell paint), the skeleton
  markers — is untouched.
- No `.ini` mutation, no config key, no persistence, no new dependency, no new crate
  feature. One new `CellSpec` shape discriminant and one new pure resolver.
- ARCHITECTURE.md and DESIGN.md are derived artifacts: they regenerate with
  `/make-architecture` and `/make-design` at archive time, never by hand. The
  capability specs sync at archive.
- Review point: the isotropic mapping is the riskiest visual change. Review the
  re-rendered `module-ui` shot before judging the composed-face work, because the
  whole plate changes proportion.

## Non-goals

- No button-face parity for P4B2, master, G8 or X7. The rule and the code path are
  the same, but those controllers keep the glyph rendering and their LEDs keep
  today's standalone cells; extending the fold table is the follow-up.
- No E4/DB8E LED rings and no P8S8 slider-cap LEDs (bead `droid_tui-xbm`).
- No patch-driven LED colour. Nothing yet constructs `ComponentState::Led { value,
  rgb }`, so an RGB device's fill renders white; modelling the colour registers is a
  separate change, filed as a follow-up bead rather than fixed here.
- No shift colour on the module UI (`cell_visuals` still hard-codes shift off). The
  new ring is the natural place for it; filed as a follow-up.
- No change to `controller_geometry.json` or `tools/acquire_geometry.py`. The fold
  shape comes from a per-controller rule, not from the geometry's currently unread
  `kind: "touch"` annotation, so bead `droid_tui-a0f` stays as it is.
- No change to the fit contract, the zoom presets, rack packing, fold bars, mounts,
  element navigation order, or the DB8E display band.
- No hand-edits to ARCHITECTURE.md or DESIGN.md.

## New types (YAGNI consumer check)

- `CellShape` (`Round` | `Rect`) on `CellSpec`: set by `physical_spec` and consumed
  by `paint_cell` to choose the composed-face branch; its absence keeps today's glyph
  path for every other element kind and controller.
- `Patch::folded_led_for_element`: consumed by `physical_spec` (the fold and the
  suppression) and by the handler's chord.
- No other new type, field, variant, or derive.
