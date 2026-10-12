## MODIFIED Requirements

### Requirement: Faceplate element fidelity

The module UI SHALL render each controller as a faceplate containing every interactive hardware element of that controller type — pots, faders, encoders, buttons, switches, jacks — at its faceplate position and with the element rendering its type calls for (a knob with pointer, a vertical fader track with thumb, an encoder, a composed button face per "Button face and LED composition", a rotary or toggle switch, or a jack). Where the panel arranges an element family as a single vertical column, the faceplate SHALL render that family as one column at the panel's measured pitch; where the panel arranges a family as a matrix, the faceplate SHALL keep the panel's rows and columns. Faceplate surface artwork SHALL NOT be rendered; the faceplate background is plain.

#### Scenario: M4 fader renders as a fader

- **WHEN** a patch declares an M4 controller
- **THEN** each of its four faders renders as a vertical track with a thumb at the fader's value, not as a knob

#### Scenario: S10 distinguishes rotary and toggle switches

- **WHEN** a patch declares an S10 controller
- **THEN** its two rotary switches render with a rotary glyph and its eight 3-position switches render with a toggle glyph

#### Scenario: B32 keeps its 4×8 grid

- **WHEN** a patch declares a B32 controller
- **THEN** its 32 buttons render in 4 columns and 8 rows in faceplate order

#### Scenario: P2B8 pots stack vertically

- **WHEN** a patch declares a P2B8 controller
- **THEN** its two pots render one above the other in a single centred column at the panel's pitch, not side by side

#### Scenario: E4 encoders stack vertically

- **WHEN** a patch declares an E4 controller
- **THEN** its four encoders render one above the other in a single centred column, each with its LED ring around it

#### Scenario: P10 keeps its small-pot matrix

- **WHEN** a patch declares a P10 controller
- **THEN** its two big pots render in a single centred column and its eight small pots render in the panel's 2-column × 4-row matrix

#### Scenario: DB8E encoder sits below its buttons

- **WHEN** a patch declares a DB8E controller
- **THEN** its encoder renders below the button grid with its LED ring, not beside the buttons

### Requirement: LEDs fold into their element

An LED that belongs to an element SHALL render as part of that element and SHALL NOT render as a standalone cell. The binding SHALL follow the controller's physical design: B32, P2B8, and P4B2 button LEDs light the button face; each M4 LED lights the touch plate below its fader; each E4 encoder carries a ring of 32 LEDs around it; P8S8 slider LEDs light the slider track; master, G8, and X7 LEDs render beside their jack. An explicit LED pairing in the patch (`led = L.N` or a same-suffix `ledN = L.M`) SHALL override the positional default. Where the LED lights the element's own face — the P2B8 and B32 button faces and the M4 touch plate — the fold SHALL render as the element face's inner fill per "Button face and LED composition", and that LED SHALL NOT publish a hit rect of its own.

#### Scenario: B32 has no LED cells

- **WHEN** a patch declares a B32 with its LEDs
- **THEN** the B32 faceplate renders 32 button elements and zero standalone LED cells, and each button shows its LED state

#### Scenario: M4 LED on the touch plate

- **WHEN** a patch drives an M4 LED
- **THEN** the LED renders as a fill inside the touch plate of the matching fader, not as a cell below it

#### Scenario: Explicit pairing overrides position

- **WHEN** a circuit section pairs `button11 = B1.3` with `led11 = L1.7`
- **THEN** L1.7 renders on B1.3

#### Scenario: Folded LED is not hit-testable

- **WHEN** the module UI publishes its element hit rects for a P2B8, B32 or M4 faceplate
- **THEN** no published rect belongs to a folded LED, and the LED's element cell is the only target over that area

### Requirement: LED colour and brightness come from the patch

LED rendering SHALL reflect the brightness and colour the patch writes to the LED registers. Controllers with RGB LEDs (M4, E4, master, G8, DB8E) SHALL render the patch-set colour; controllers with white-only LEDs (B32) SHALL render brightness only. An LED with no colour written SHALL render in the device's default colour. A folded LED's fill extent and colour SHALL both follow that level.

#### Scenario: RGB colour on M4

- **WHEN** a patch sets an M4 touch-plate LED to a colour
- **THEN** the touch plate renders in that colour

#### Scenario: White-only B32

- **WHEN** a patch writes a colour to a B32 button LED
- **THEN** the button renders the LED brightness in white

#### Scenario: Level drives the fill extent

- **WHEN** a folded LED's register goes from dark to lit
- **THEN** the fill inside its element's face grows from the floor extent to the full face extent

### Requirement: Module UI interactions

The module UI SHALL support the element interactions the Panels view provided: keyboard navigation between elements, Enter/Space to toggle or select, `Ctrl+Enter` to toggle the focused element's folded LED without changing the element's own state, mouse wheel to adjust knobs and faders, holding and latching a modifier (`m`, `Ctrl+Shift+Click`) with the modifier wash, the label editor (`e`), shift groups `1`–`4`, and highlighting the hardware of the selected circuit.

#### Scenario: Label editor from the module UI

- **WHEN** an element is focused in the module UI and the user presses `e`
- **THEN** the label editor opens for that element's token

#### Scenario: Wheel adjusts a fader

- **WHEN** the pointer is over a fader element and the user scrolls
- **THEN** the fader value changes

#### Scenario: LED toggle leaves the element state alone

- **WHEN** an element with a folded LED is focused and the user presses `Ctrl+Enter`
- **THEN** the folded LED's state toggles and the element's own state is unchanged

## ADDED Requirements

### Requirement: Button face and LED composition

A button-kind element whose controller lights its LED in the button face SHALL render as a state shape instead of a state glyph: an outline whose colour carries the button's on/off state, with the folded LED drawn as an inner fill whose extent and colour follow the LED level. A round face (P2B8, B32) SHALL render a circle and a rectangular face (the M4 touch plate) SHALL render a rectangle. The element's label SHALL render under the shape when the cell has room for it; the element's state text SHALL NOT render in the cell, because the shape carries the state. An element with no bound LED SHALL render the shape alone. A button on any other controller SHALL keep the glyph rendering.

#### Scenario: P2B8 button renders a state circle

- **WHEN** a patch declares a P2B8 button
- **THEN** its cell renders a circle whose outline colour follows the button's on/off state, with no state glyph and no state-text row

#### Scenario: B32 button LED fills the circle

- **WHEN** a B32 button's LED is at full level
- **THEN** the circle is filled to its full extent in the LED's brightness, and at the floor level it shows only a small dim core

#### Scenario: M4 touch plate fills as a rectangle

- **WHEN** a patch declares an M4 touch plate
- **THEN** its cell renders a rectangle outline with the LED as an inner rectangle fill, not a circle

#### Scenario: Button without an LED

- **WHEN** a button on a composed controller has no bound LED
- **THEN** the cell renders the state shape alone with no fill

#### Scenario: Label under a roomy shape

- **WHEN** a composed cell is tall enough for a label line beneath the shape
- **THEN** the label renders under the shape at the fixed cell-label size, clipped to the cell width

#### Scenario: Out-of-scope controller keeps its glyph

- **WHEN** a patch declares a button on a controller whose LED does not light the button face (P4B2, master, G8, X7)
- **THEN** the cell renders the state glyph as before
