## MODIFIED Requirements

### Requirement: LED colour and brightness come from the patch

LED rendering SHALL reflect the brightness and colour the patch writes to the LED registers. Controllers with RGB LEDs (M4, E4, master, G8, DB8E) SHALL render the patch-set colour; controllers with white-only LEDs (B32) SHALL render brightness only. An LED with no colour written SHALL render in the device's default colour. A folded LED's fill extent and colour SHALL both follow that level.

A static colour source SHALL be resolved at load from the owning circuit section's `ledcolor*` param (same-suffix pairing first, else the section's single `ledcolor` for single-LED circuits) and brightness from `ledvalue*` the same way. A source value SHALL resolve when it is a float literal, a constant expression, or a single-hop cable ref to a constant producer output; anything dynamic SHALL fall back to the previous rendering (white fill at the floor). Toggling an LED SHALL preserve its resolved colour.

#### Scenario: RGB colour on M4

- **WHEN** a patch sets an M4 touch-plate LED to a colour
- **THEN** the touch plate renders in that colour

#### Scenario: White-only B32

- **WHEN** a patch writes a colour to a B32 button LED
- **THEN** the button renders the LED brightness in white

#### Scenario: Level drives the fill extent

- **WHEN** a folded LED's register goes from dark to lit
- **THEN** the fill inside its element's face grows from the floor extent to the full face extent

#### Scenario: Static ledcolor resolves at load

- **WHEN** a patch sets `ledcolor = 0.5` with `ledvalue = 1` on a motorfader section driving a touch plate
- **THEN** the plate's LED loads in the mapped colour at full brightness instead of white

#### Scenario: Dynamic colour falls back

- **WHEN** a patch wires `ledcolor` to a cable driven by a live circuit output
- **THEN** the LED renders as before (white fill at the floor), with no error

#### Scenario: Toggle keeps the colour

- **WHEN** a coloured LED is toggled via `Enter`, `Space`, or `Ctrl+Enter`
- **THEN** its brightness flips and its resolved colour is unchanged
