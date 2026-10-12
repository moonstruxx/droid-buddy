## MODIFIED Requirements

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
