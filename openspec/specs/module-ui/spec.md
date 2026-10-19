# module-ui Specification

## Purpose
The module UI renders the patch's DROID controllers as simplified faceplates in rack order, showing every real interactive element at its physical position with its live state, its LED, and its shift-aware label.

## Requirements

### Requirement: Faceplate element fidelity

The module UI SHALL render each controller as a faceplate containing every interactive hardware element of that controller type — pots, faders, encoders, buttons, switches, jacks — at its faceplate position and with the element rendering its type calls for (a knob with pointer, a vertical fader track with thumb, an encoder, a composed button face per "Button face and LED composition", a rotary or toggle switch, or a jack). Where the panel arranges an element family as a single vertical column, the faceplate SHALL render that family as one column at the panel's measured pitch; where the panel arranges a family as a matrix, the faceplate SHALL keep the panel's rows and columns. Faceplate surface artwork SHALL NOT be rendered; the faceplate background is plain. Switch-kind annotations in the controller geometry (`rotary` vs toggle) SHALL drive the switch glyph: rotary switches render round, toggles square.

#### Scenario: M4 fader renders as a fader

- **WHEN** a patch declares an M4 controller
- **THEN** each of its four faders renders as a vertical track with a thumb at the fader's value, not as a knob

#### Scenario: S10 distinguishes rotary and toggle switches

- **WHEN** a patch declares an S10 controller
- **THEN** its two rotary switches render with a rotary glyph and its eight 3-position switches render with a toggle glyph

#### Scenario: Unannotated switches keep the toggle glyph

- **WHEN** a switch cell carries no kind annotation
- **THEN** it renders with the toggle glyph exactly as before this change

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

### Requirement: Unused elements are drawn

The module UI SHALL render every hardware element of a declared controller, including elements the patch does not reference. Unreferenced elements SHALL render dimmed and without a label. Unreferenced elements SHALL NOT publish hit rects and SHALL NOT be reachable by keyboard navigation, selection, the label editor, or modifier latching. An unreferenced LED on a fold-rule controller (P2B8, B32, M4) SHALL NOT render as a standalone cell (the fold rule covers its face); all other unreferenced LEDs SHALL render dimmed standalone.

#### Scenario: Partly used B32

- **WHEN** a patch uses 5 of a B32's 32 buttons
- **THEN** all 32 buttons render; the 27 unused buttons are dimmed and unlabelled

#### Scenario: Unreferenced cells are not hit-testable

- **WHEN** the module UI publishes its element hit rects for a partially used faceplate
- **THEN** no published rect belongs to an unreferenced element

#### Scenario: Unreferenced button LED stays folded

- **WHEN** a P2B8 button and its LED are both unreferenced by the patch
- **THEN** the face renders one dimmed button cell and no standalone LED cell

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

### Requirement: E4 rings follow the LED registers

Each E4 encoder's ring SHALL render the 32 LED registers assigned to that encoder (L1.1–L1.32 for the first encoder, L1.33–L1.64 for the second, and so on), each segment showing its register's brightness and colour. The ring SHALL NOT be derived from the encoder's value. Ring segments the patch does not reference SHALL render dimmed.

#### Scenario: Patch lights part of a ring

- **WHEN** a patch lights L1.1–L1.8 and leaves L1.9–L1.32 dark
- **THEN** the first encoder's ring shows eight lit segments and 24 dark segments

#### Scenario: Bare E4 shows full dimmed rings

- **WHEN** a patch declares a bare `[e4]` (synthesizing 4 encoders, 4 buttons, 4 LEDs)
- **THEN** all 128 ring segments render, the referenced ones lit and the rest dimmed and unlabelled

### Requirement: Master faceplate

The master and master18 SHALL render as faceplates in their rack position with their jacks, LEDs, and button, replacing any separate CV I/O group. Input and output jacks SHALL be distinguishable.

#### Scenario: Master in the rack

- **WHEN** a patch uses master inputs and outputs
- **THEN** a master faceplate renders at the start of the rack with its 4×4 jack grid, its LEDs beside the jacks, and no separate CV I/O panel

### Requirement: Shift-aware element labels

Every element the patch references SHALL show its label, resolved for the active shift layer with the existing label fallback (stored label for the layer, then layer 1, then the patch preamble, then the derived name). Changing the shift layer SHALL update every label immediately. A label SHALL render inside its cell at the fixed cell-label font when the cell is wide enough for at least one character of that font and SHALL be ellipsized to the cell width; when the cell cannot hold the label, the element's label SHALL remain reachable through the label presentation (its annotation callout) and through the hover reveal, so no addressed element's label is unavailable at any zoom. Text SHALL NOT be rendered below 8 pt anywhere on the surface.

#### Scenario: Shift changes labels

- **WHEN** B1.1 has different labels for Group 1 and Group 2 and the user presses `2`
- **THEN** B1.1 shows its Group 2 label

#### Scenario: Long label on hover

- **WHEN** an element's label is ellipsized or hidden and the pointer hovers the element
- **THEN** the full label appears in the hover callout and in the status line of the module-UI pane

#### Scenario: Narrow cell hides the label in-cell but not from the user

- **WHEN** a cell is narrower than one character of the label font at any zoom preset
- **THEN** the cell draws its state without text, and the element's full label is available through the hover reveal in every label-presentation mode

### Requirement: Fit-width default

The module UI SHALL fit the rack to its pane — scaling so the whole rack width AND height are visible, and centring the rack in the pane — when the view opens, when a patch loads, when the rack definition changes, and when the pane is resized. After the user zooms or pans the view, these triggers SHALL leave the view alone until the user requests a fit again. A fit request SHALL always be available and SHALL apply the same computation.

#### Scenario: Rack fits the pane on open

- **WHEN** the module UI opens in a pane
- **THEN** the full rack is visible without panning, centred, and the fit zoom is reported

#### Scenario: Manual navigation is preserved

- **WHEN** the user zooms or pans the module UI and then the pane is resized
- **THEN** the zoom and pan the user set are unchanged

#### Scenario: Fit applies on request

- **WHEN** the user requests a fit after zooming and panning
- **THEN** the rack is visible in full and centred again

### Requirement: Module UI interactions

The module UI SHALL support the element interactions the Panels view provided: keyboard navigation between elements, Enter/Space to toggle or select, `Ctrl+Enter` to toggle the focused element's folded LED without changing the element's own state, mouse wheel to adjust knobs and faders, holding and latching a modifier (`m`, `Ctrl+Shift+Click`) with the modifier wash, the label editor (`e`), shift groups `1`–`4`, and highlighting the hardware of the selected circuit. The pointer gestures SHALL be disambiguated by the hit cell and the modifier: a wheel over an adjustable element adjusts its value, a wheel over the pane but not over an adjustable element zooms about the pointer, `Ctrl`+wheel zooms about the pointer wherever the pointer is, and a middle-button drag pans the view. None of these gestures SHALL change which element is selected or focus.

#### Scenario: Label editor from the module UI

- **WHEN** an element is focused in the module UI and the user presses `e`
- **THEN** the label editor opens for that element's token

#### Scenario: Wheel adjusts a fader

- **WHEN** the pointer is over a fader element and the user scrolls
- **THEN** the fader value changes

#### Scenario: Wheel off an element zooms

- **WHEN** the pointer is over the pane but not over an adjustable element and the user scrolls
- **THEN** the view zooms about the pointer

#### Scenario: Modified wheel zooms over an element

- **WHEN** the pointer is over an adjustable element and the user scrolls with `Ctrl` held
- **THEN** the view zooms about the pointer and the element's value is unchanged

#### Scenario: Middle-drag pans

- **WHEN** the user drags with the middle button over the module UI
- **THEN** the view pans and no element is selected or changed

#### Scenario: LED toggle leaves the element state alone

- **WHEN** an element with a folded LED is focused and the user presses `Ctrl+Enter`
- **THEN** the folded LED's state toggles and the element's own state is unchanged

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

### Requirement: Label presentation modes

The module UI SHALL provide a label presentation with three modes — `off`, `auto`, and `on` — cycled by a user action and persisted as a `[physical]` setting whose absent value is `auto`.

- In `off` the view SHALL render labels inside their cells only, as the compact-cell contract describes.
- In `auto` the view SHALL annotate exactly those addressed elements whose in-cell label is clipped or hidden at the current zoom and whose label is explicitly set (a stored label or a patch preamble label) — never an element whose label already fits inside its cell, never an element of the patch that the controller does not address, and never an element whose label is only the derived token name. Annotations SHALL be drawn outside the rack with a leader line to the annotated cell and SHALL use the same presentation as the performance view's callouts. When the deepest annotation band would exceed 30 % of the pane on either axis, the view SHALL drop annotations in a fixed priority order until the bands fit: LEDs and jacks first, then buttons, then switches, then pots, then faders, and never the focused element or the hardware of the selected circuit.
- In `on` the view SHALL annotate every addressed element that carries a label, with no dropping.

Annotations SHALL be decorative: they SHALL publish no hit rect, SHALL NOT alter the drawn cell rects or the published component hit rects, and SHALL NOT change what the skeleton presentation draws. The view SHALL report how many labels are displayed and, when any were dropped or hidden, how many remain available through the hover reveal.

#### Scenario: Auto annotates only what the cell cannot show

- **WHEN** the label presentation is `auto` at true 1:1 and the rack holds a 12 mm pot whose label is short and a 9 mm button whose label is long
- **THEN** the pot renders its label inside its cell with no annotation, and the button is annotated outside the rack with a leader line to its cell

#### Scenario: Derived labels stay reveal-only

- **WHEN** the label presentation is `auto` and an element carries no stored or preamble label, only its derived token name
- **THEN** the element gets no annotation even though its cell cannot show the derived name, and the full name still appears through the hover reveal

#### Scenario: Unaddressed elements are never annotated

- **WHEN** the controller declares elements the patch does not address
- **THEN** those elements render dimmed and unlabelled, and no annotation names them

#### Scenario: Budget drops in priority order

- **WHEN** a dense patch's annotation bands would exceed 30 % of the pane height
- **THEN** annotations are dropped starting with LEDs and jacks, the focused element and the selected circuit's hardware keep theirs, and the status reports the displayed and hidden counts

#### Scenario: Off keeps the cells alone

- **WHEN** the label presentation is `off`
- **THEN** no annotation is drawn and only in-cell labels appear

#### Scenario: On annotates everything labelled

- **WHEN** the label presentation is `on`
- **THEN** every addressed element with a label is annotated, whether or not its cell could hold it

#### Scenario: Annotations are decoration

- **WHEN** annotations are drawn over a patch
- **THEN** the published component hit rects and the skeleton rects are byte-identical to the same view with the presentation `off`

#### Scenario: Presentation survives a patch load

- **WHEN** the user selects a presentation and loads another patch
- **THEN** the selected presentation still applies

### Requirement: Element label reveal

The module UI SHALL reveal the full label of the element under the pointer, together with its state, as a callout anchored to that element's cell, in every label-presentation mode and in both the full and skeleton presentations. The callout SHALL be positioned inside the pane (flipping side near a pane edge), SHALL publish no hit rect, and SHALL NOT hide the element it describes. With no element under the pointer no callout SHALL be drawn. The same information SHALL be reported in the module-UI pane's status line for the element the pointer is over, falling back to the focused element when the pointer is elsewhere.

#### Scenario: Hover reveals a clipped label

- **WHEN** the label presentation is `off` and the pointer hovers an element whose label does not fit its cell
- **THEN** a callout shows the element's full label and state beside its cell

#### Scenario: Callout stays inside the pane

- **WHEN** the pointer hovers an element at the pane's right edge
- **THEN** the callout is drawn on the side of the cell that keeps it inside the pane

#### Scenario: Leaving the pane clears the reveal

- **WHEN** the pointer leaves the module-UI pane
- **THEN** no callout is drawn
