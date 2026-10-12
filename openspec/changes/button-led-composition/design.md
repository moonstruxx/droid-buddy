# Design

## Context

See `proposal.md` - Why for the motivation and the two measurements. The current
state that shapes the approach:

- `src/physical.rs` owns `ScreenMapping` (mm → screen as
  `screen = mm × factor × zoom − offset`) with two per-axis factors,
  `PHYSICAL_COLS_PER_MM = 0.15` and `PHYSICAL_ROWS_PER_MM = 0.3`.
  `src/gui/physical.rs::rack_geometry` then multiplies both axes by one
  `PHYSICAL_CELL_SCALE = 10.0` points per cell — that single uniform scale is what
  turns the per-axis factors into a 2:1 vertical stretch in the native window.
- `physical_spec` builds a private `element_leds` map (explicit `comp.led` plus
  `patch::positional_leds_for_element`) and a `folded_leds` set that nothing reads;
  `rack_geometry` emits a cell for every component whose token resolves in the
  geometry, so LED tokens draw as cells and publish hit rects.
- `paint_cell` draws the "fold" as a `led_ring` square in the cell's top-right
  corner, ignoring the LED's own geometry, and the compact path draws the state
  glyph, the label and the state text from `cell_visuals`.
- LED components are created `Off` by the parser and only the UI toggles them;
  `ComponentState::Led { value, rgb }` and `ComponentState::Active` are never
  constructed, so `led_rgb` is always `None`.
- The element hit contract is `component_rects`, republished each frame from the
  Physical pane's painted cells; the window pointer path resolves hover and click
  through it.

## Goals / Non-Goals

**Goals:**

- One honest mm→screen scale, so a round button can be drawn as a circle and the
  plate's proportions are true.
- A button face that reads as a button: state on the outline, the folded LED as the
  inner fill, one cell, one hit rect, no duplicate LED rendering.
- One source of truth for the element→LED binding, shared by the painter and the
  handler.
- Keep the LED reachable from the keyboard without giving it a cell.

**Non-Goals:**

- Everything in the proposal's Non-goals (controller scope, LED colour source, E4/
  DB8E rings, shift colour, geometry-data changes, doc hand-edits).
- No new interaction concept: the LED chord is an extra binding on the existing
  component toggle path, not a new mode or overlay.

## Decisions

### D1 — One isotropic factor replaces the two per-axis factors

`ScreenMapping` carries a single `cells_per_mm`, and `PHYSICAL_CELLS_PER_MM = 0.15`
replaces the `PHYSICAL_COLS_PER_MM`/`PHYSICAL_ROWS_PER_MM` pair.
`PHYSICAL_CELL_SCALE` stays the points-per-cell step, so `rack_geometry`'s rect
math, the pane-origin conversion and `mm_grid_lines` keep their shape.

*Why:* a point is a point. The two per-axis factors only balanced in a hypothetical
terminal whose cell was twice as wide as tall; the native window has no cells, so
the pair had become two names for one fact with a 2× stretch as a side effect.

*Alternatives:* keep both fields with equal values (leaves a vestigial concept and a
trap for the next reader); collapse `cells_per_mm` and `PHYSICAL_CELL_SCALE` into a
single points-per-millimetre constant (more honest, but it churns every consumer of
`cell_scale` — the pane-origin math, `PhysicalSpec`, the performance mapping — for
no behaviour change).

*Consequence:* fit stays width-only as specified, and the drawn rack's height
halves, so the vertical overflow shrinks but does not disappear for a narrow chain.
Contain-fit (fit both axes) would remove it; out of scope.

### D2 — The fold is a per-controller rule, not a geometry annotation

A pure rule keyed on the controller geometry key decides whether an LED folds into
its element's face and which shape that face has: `p2b8`/`b32` → round, `m4` → rect,
every other controller → no fold.

*Why:* the binding follows the controller's physical design (the `module-ui`
requirement lists it controller by controller), and the face shape follows the same
design. The geometry file already declares `"kind": "touch"` for M4 plates, but that
annotation reaches neither the model (`ElementCellEntry` has no `kind` field — bead
`droid_tui-a0f`) nor the fold decision, which is per-controller anyway.

*Alternatives:* carry `kind` from the geometry into `ElementCell`/`CellSpec` (one
serde field, and it would close a known gap — but it answers a question the fold
rule already answers, so it belongs in the change that needs it, `a0f`); infer the
shape from a cell's aspect ratio (implicit and fragile).

### D3 — One binding resolver in `patch.rs`

`Patch::folded_led_for_element(&HwComponent) -> Option<String>` returns the explicit
`led` pairing when the parser recorded one, otherwise the first
`positional_leds_for_element(controller, token)` entry.

*Why:* the painter needs it for the fill and for the suppression, and the handler
needs exactly the same answer for the chord; the private copy inside `physical_spec`
would drift.

*Alternatives:* duplicate the two-line resolution in the handler (drift risk); put
it on `App` (the binding is patch data, and `App` would only proxy it).

### D4 — Suppression happens in `physical_spec`, not in `rack_geometry`

`physical_spec` drops the cells of folded LEDs when it builds `CellSpec`s;
`rack_geometry` keeps emitting geometry for every resolvable token.

*Why:* the fold depends on the patch's LED bindings, which the pure rack geometry
does not know; and a single filter site keeps the skeleton and the full render in
agreement, which the `physical-scale-model` coincidence contract requires. Because
`paint_panes` publishes `component_rects` from the specs it just painted, the
suppression removes the LED hit rects with no second change.

*Alternatives:* teach `rack_geometry` to skip the `L` family (it cannot know which
LEDs are folded, and it would also drop the LED cells that stay standalone for
E4/DB8E/P8S8/master).

### D5 — The fill is the face's inner shape scaled by level, with the existing floor

The LED level (0..1, from `led_state`) scales the inner shape:
`extent = floor + (1 − floor) × level` with the existing 25 % floor, and the fill
colour is the patch RGB scaled by the same level when known, otherwise white. Round
faces get a concentric disc; the M4 plate gets a concentric rectangle.

*Why:* it is the direct reading of "the LED fills that circle", it mirrors cleanly
between the two shapes, and it preserves the `fader-led-contrast` intent — an off LED
still shows a field, nothing goes black-on-black.

*Alternatives:* a full-size fill whose brightness tracks the level (loses the "how
full is it" reading and makes the button state and the LED compete for one channel);
a bottom-up bar on the M4 plate (a level meter, not the hardware).

### D6 — The outline carries the button state

The face outline is the state channel: `muted` when the button is off, the `button`
token with a slightly bolder stroke when it is on; the interior belongs to the LED.
The paused dim applies to the outline and the label while the LED fill stays bright,
matching the existing fader/LED exemption.

*Why:* the interior is spoken for by the fill, so the state has to live on the
outline; this also keeps the state legible when the fill is at full extent (the
classic palette puts `button` and `led` on different colours).

*Alternatives:* encode the state by fill alpha only (invisible when the LED is
dark); keep the `●`/`○` glyph inside the circle (two state channels fighting inside
a 13.5 pt circle).

### D7 — Label rule for composed faces

A composed cell drops the state-text row and draws its label under the shape when
the cell has room: the shape takes `cell_h − label_line` when that still leaves at
least half the cell width, otherwise the shape fills the cell and the label is
omitted (hover and the status bar keep the full label, as they already do for narrow
cells). The label keeps the fixed `CELL_LABEL_SIZE` font and is clipped to the cell
width.

*Why:* the label-size contract is explicitly zoom-independent, so the room has to be
found in geometry rather than in a smaller font. At the 200 % preset a P2B8 cell is
27 × 27 pt, which fits a ~14.5 pt circle plus a label line; at 100 % a 13.5 pt cell
does not, so the label drops — roughly what today's one or two clipped characters
amount to.

*Alternatives:* shrink the label font (breaks "Label size independent of physical
zoom"); draw the label inside the circle (fights the fill and the ring).

### D8 — `Ctrl+Enter` as the LED handle

`Ctrl+Enter` toggles the hovered element's folded LED and reports it; with no folded
LED it is a silent no-op, and while processing is paused it is a no-op like the
existing toggles.

*Why:* the key is unbound, Ctrl survives the winit key path (`Ctrl+C` already quits
through it), and leaving `Enter`/`Space` on the element preserves the interaction
users know. `j`/`k` and clicks stop visiting LEDs, which is the point of removing
their cells.

*Alternatives:* `Shift+Enter` (Shift is overloaded by pane and viewer actions);
latching the LED from hover (cannot reach a non-hovered cell, and it would need a
new state); leaving LEDs untogglable until register simulation exists (the fill
would never move, so the feature could not be seen at all).

## Risks / Trade-offs

- [The isotropic fix changes the whole plate's proportions, and a
  screenshot-based review may read it as a regression] → it is the point of the
  change; review the re-rendered `module-ui` shot first, and re-derive (never
  relax by hand) the tests that encode the old 2:1 numbers —
  `rack_geometry_maps_mm_to_points`, `mm_grid_lines_span_the_rack_at_step_intervals`,
  the `PHYSICAL_ROWS_PER_MM` assertions in `src/physical.rs`, and `screen_mapping_*`.
- [Composed faces lose the inline label at the 75 %/100 % presets, which reads as
  information loss] → the label was already clipped to one or two characters at
  those presets; the full label stays on hover, in the status bar and in the source
  pane, and the shape is a larger hit target than the old glyph.
- [Suppressing LED cells removes LEDs from `component_rects`, so a click that used
  to land on an LED now lands on the element] → intended; `Ctrl+Enter` is the
  keyboard replacement, and the element's selection and influence behaviour is
  unchanged.
- [The fill is white on RGB devices until the colour registers are modelled] →
  documented as a non-goal and filed as a follow-up bead so the gap stays visible.
- [P4B2 (and master/G8/X7) buttons keep the glyph rendering beside composed faces,
  so the module UI briefly speaks two button languages] → deliberate scope boundary;
  the fold table makes the extension one line plus a test row.

## Open Questions

- Whether the label should later move inside the composed face (a legend on the
  button) instead of under it. Deferrable: the rule above is a geometry decision the
  spec pins, and changing it later changes only the paint routine, not the specs or
  the task breakdown.
