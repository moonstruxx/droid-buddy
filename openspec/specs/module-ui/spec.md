# module-ui Specification

## Purpose
The module UI renders the patch's DROID controllers as simplified faceplates in rack order, showing every real interactive element at its physical position with its live state, its LED, and its shift-aware label.

## Requirements

### Requirement: Faceplate element fidelity

The module UI SHALL render each controller as a faceplate containing every interactive hardware element of that controller type — pots, faders, encoders, buttons, switches, jacks — at its faceplate position and with a glyph for its element type (knob with pointer, vertical fader track with thumb, encoder, button, rotary switch, toggle switch, jack). Faceplate surface artwork SHALL NOT be rendered; the faceplate background is plain.

#### Scenario: M4 fader renders as a fader

- **WHEN** a patch declares an M4 controller
- **THEN** each of its four faders renders as a vertical track with a thumb at the fader's value, not as a knob

#### Scenario: S10 distinguishes rotary and toggle switches

- **WHEN** a patch declares an S10 controller
- **THEN** its two rotary switches render with a rotary glyph and its eight 3-position switches render with a toggle glyph

#### Scenario: B32 keeps its 4×8 grid

- **WHEN** a patch declares a B32 controller
- **THEN** its 32 buttons render in 4 columns and 8 rows in faceplate order

### Requirement: Unused elements are drawn

The module UI SHALL render every hardware element of a declared controller, including elements the patch does not reference. Unreferenced elements SHALL render dimmed and without a label.

#### Scenario: Partly used B32

- **WHEN** a patch uses 5 of a B32's 32 buttons
- **THEN** all 32 buttons render; the 27 unused buttons are dimmed and unlabelled

### Requirement: LEDs fold into their element

An LED that belongs to an element SHALL render as part of that element and SHALL NOT render as a standalone cell. The binding SHALL follow the controller's physical design: B32, P2B8, and P4B2 button LEDs light the button face; each M4 LED lights the touch plate below its fader; each E4 encoder carries a ring of 32 LEDs around it; P8S8 slider LEDs light the slider track; master, G8, and X7 LEDs render beside their jack. An explicit LED pairing in the patch (`led = L.N` or a same-suffix `ledN = L.M`) SHALL override the positional default.

#### Scenario: B32 has no LED cells

- **WHEN** a patch declares a B32 with its LEDs
- **THEN** the B32 faceplate renders 32 button elements and zero standalone LED cells, and each button shows its LED state

#### Scenario: M4 LED on the touch plate

- **WHEN** a patch drives an M4 LED
- **THEN** the LED renders on the touch plate below the matching fader

#### Scenario: Explicit pairing overrides position

- **WHEN** a circuit section pairs `button11 = B1.3` with `led11 = L1.7`
- **THEN** L1.7 renders on B1.3

### Requirement: LED colour and brightness come from the patch

LED rendering SHALL reflect the brightness and colour the patch writes to the LED registers. Controllers with RGB LEDs (M4, E4, master, G8, DB8E) SHALL render the patch-set colour; controllers with white-only LEDs (B32) SHALL render brightness only. An LED with no colour written SHALL render in the device's default colour.

#### Scenario: RGB colour on M4

- **WHEN** a patch sets an M4 touch-plate LED to a colour
- **THEN** the touch plate renders in that colour

#### Scenario: White-only B32

- **WHEN** a patch writes a colour to a B32 button LED
- **THEN** the button renders the LED brightness in white

### Requirement: E4 rings follow the LED registers

Each E4 encoder's ring SHALL render the 32 LED registers assigned to that encoder (L1.1–L1.32 for the first encoder, L1.33–L1.64 for the second, and so on), each segment showing its register's brightness and colour. The ring SHALL NOT be derived from the encoder's value.

#### Scenario: Patch lights part of a ring

- **WHEN** a patch lights L1.1–L1.8 and leaves L1.9–L1.32 dark
- **THEN** the first encoder's ring shows eight lit segments and 24 dark segments

### Requirement: Master faceplate

The master and master18 SHALL render as faceplates in their rack position with their jacks, LEDs, and button, replacing any separate CV I/O group. Input and output jacks SHALL be distinguishable.

#### Scenario: Master in the rack

- **WHEN** a patch uses master inputs and outputs
- **THEN** a master faceplate renders at the start of the rack with its 4×4 jack grid, its LEDs beside the jacks, and no separate CV I/O panel

### Requirement: Shift-aware element labels

Every element the patch references SHALL show its label, resolved for the active shift layer with the existing label fallback (stored label for the layer, then layer 1, then the patch preamble, then the derived name). Changing the shift layer SHALL update every label immediately. A label longer than the space available SHALL be ellipsized, and the full label SHALL appear on hover and in the status bar.

#### Scenario: Shift changes labels

- **WHEN** B1.1 has different labels for Group 1 and Group 2 and the user presses `2`
- **THEN** B1.1 shows its Group 2 label

#### Scenario: Long label on hover

- **WHEN** an element's label is ellipsized and the pointer hovers the element
- **THEN** the full label appears in the tooltip and the status bar

### Requirement: Fit-width default

The module UI SHALL open with the rack fitted to the width of its pane. Zoom and pan SHALL remain available after opening.

#### Scenario: Rack fits the pane on open

- **WHEN** the module UI opens in a pane
- **THEN** the full rack width is visible without panning

### Requirement: Module UI interactions

The module UI SHALL support the element interactions the Panels view provided: keyboard navigation between elements, Enter/Space to toggle or select, mouse wheel to adjust knobs and faders, holding and latching a modifier (`m`, `Ctrl+Shift+Click`) with the modifier wash, the label editor (`e`), shift groups `1`–`4`, and highlighting the hardware of the selected circuit.

#### Scenario: Label editor from the module UI

- **WHEN** an element is focused in the module UI and the user presses `e`
- **THEN** the label editor opens for that element's token

#### Scenario: Wheel adjusts a fader

- **WHEN** the pointer is over a fader element and the user scrolls
- **THEN** the fader value changes
