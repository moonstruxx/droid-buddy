# Spec Delta

## MODIFIED Requirements

### Requirement: Faceplate element fidelity

The module UI SHALL render each controller as a faceplate containing every interactive hardware element of that controller type — pots, faders, encoders, buttons, switches, jacks — at its faceplate position and with the element rendering its type calls for (a pot face, a vertical fader track with thumb, an encoder face, a composed button face per "Button face and LED composition", a rotary or toggle switch, or a jack face). Where the panel arranges an element family as a single vertical column, the faceplate SHALL render that family as one column at the panel's measured pitch; where the panel arranges a family as a matrix, the faceplate SHALL keep the panel's rows and columns. Faceplate surface artwork SHALL NOT be rendered; the faceplate background is plain. Switch-kind annotations in the controller geometry (`rotary` vs toggle) SHALL drive the switch glyph: rotary switches render round, toggles square.

A rotary control SHALL render as a frontal face, not as a glyph: a washer ring enclosing a hex nut, a shaft and a slot. The slot SHALL carry the element's value on a 270° sweep, with the value at 0 at the lower-left and at 1 at the lower-right. A knob SHALL seat that face on a horizontal base line, marking a finite control with end stops; an encoder SHALL render the same face without the base line, marking an endless control. A CV jack SHALL render as a frontal socket: a bezel ring enclosing a filled mouth with a cross cut into it.

Symbol detail SHALL follow the drawn face diameter, so the faceplate stays legible at every zoom: at or above 18 pt the full symbol; from 10 pt below 18 pt the rotary face drops the nut, the shaft and the base line to a ring plus a slot; below 10 pt a rotary renders a ring plus a centre dot. Pot and encoder SHALL converge at the reduced sizes; the label and state row remain the channels that tell them apart there.

A face SHALL occupy its cell and SHALL NOT shrink or be displaced to make room for a label: the label SHALL be fitted to the band beneath the face, `h − 0.66 w`, and the state row SHALL keep its place only when that band still holds it after the labelled lines. A face SHALL NOT change the cell's published rect, its hit behaviour, its keyboard reachability, or the wheel value step.

CV direction SHALL be carried by the `◀`/`▶` corner marker and the `CV IN`/`CV OUT` state row, and SHALL NOT depend on colour alone, because the `mono` palette maps the CV in and out tokens to the same value.

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

#### Scenario: A 12 mm pot draws its full face

- **WHEN** a P2B8 pot is drawn at true size, a 45 pt cell whose face is at least 18 pt
- **THEN** it renders a washer ring, a hex nut, a shaft, a slot and a base line beneath the face, and its label takes the band under it

#### Scenario: An encoder draws the same face without a base line

- **WHEN** an E4 or DB8E encoder is drawn at true size
- **THEN** its face carries the washer ring, nut, shaft and slot and no base line, so a finite pot and an endless encoder are tellable apart by silhouette

#### Scenario: The slot carries the value at true size

- **WHEN** a pot's value is 0, 0.5 or 1 at true size
- **THEN** its slot points lower-left, straight up or lower-right respectively, and no percentage row is drawn because the band holds no room for one after the label

#### Scenario: The percentage row returns where the band allows

- **WHEN** a pot cell is drawn large enough that its band holds the label and a state row
- **THEN** the state row shows the value beside the labelled face

#### Scenario: Small cells reduce the symbol

- **WHEN** the drawn face falls below 18 pt, as at the whole-rack fit
- **THEN** the rotary renders a ring plus a slot, and below a 10 pt face a ring plus a centre dot, without losing its footprint or its hit rect

#### Scenario: A CV jack keeps its direction marker

- **WHEN** CV in and CV out cells are drawn in the `mono` palette, where both tokens are the same colour
- **THEN** each still shows its `◀` or `▶` corner marker and its `CV IN` or `CV OUT` state row, so direction is never carried by colour alone

#### Scenario: A face changes no geometry

- **WHEN** the module UI publishes its element hit rects for a faceplate of pots, encoders and jacks
- **THEN** every published rect matches the rect the same faceplate published before the faces existed, and a wheel over a pot still steps its value

### Requirement: Unused elements are drawn

The module UI SHALL render every hardware element of a declared controller, including elements the patch does not reference. Unreferenced elements SHALL render dimmed and without a label. Unreferenced elements SHALL NOT publish hit rects and SHALL NOT be reachable by keyboard navigation, selection, the label editor, or modifier latching. An unreferenced LED on a fold-rule controller (P2B8, B32, M4) SHALL NOT render as a standalone cell (the fold rule covers its face); all other unreferenced LEDs SHALL render dimmed standalone.

An unreferenced rotary or CV jack SHALL render the same symbol as an addressed one and dimmed. At the full-detail step an unreferenced rotary SHALL omit the washer ring and keep the nut, the slot and, on a pot, the base line, so it reads as unused rather than merely unlit; at the reduced and centre-dot steps it SHALL keep the addressed drawing at the dimmed floor, because a bare slot at that size no longer reads as hardware. Every other unreferenced element kind SHALL keep the plain dimmed marker. An addressed rotary that merely lacks a label SHALL still draw its full face, because the rule keys on the element having no component rather than on the absence of a label.

The master faceplate SHALL follow the same rule only when the chain comes from a configured rack module inventory: then its complete declared field renders — sixteen jacks, the `R1..R16` LED matrix and the SD button on MASTER, `O1..O8` with the `I1`/`I2` logic inputs, `G1..G4` gate outputs and the SD button on MASTER18. On a chain derived from the patch alone, the master's unreferenced cells SHALL remain suppressed, so an existing setup renders exactly as before.

When a rack module inventory is configured and no patch is loaded, the pane SHALL paint the inventory rack with every cell unaddressed, and SHALL publish no hit rects.

#### Scenario: Partly used B32

- **WHEN** a patch uses 5 of a B32's 32 buttons
- **THEN** all 32 buttons render; the 27 unused buttons are dimmed and unlabelled

#### Scenario: Unreferenced cells are not hit-testable

- **WHEN** the module UI publishes its element hit rects for a partially used faceplate
- **THEN** no published rect belongs to an unreferenced element

#### Scenario: Unreferenced button LED stays folded

- **WHEN** a P2B8 button and its LED are both unreferenced by the patch
- **THEN** the face renders one dimmed button cell and no standalone LED cell

#### Scenario: An inventoried master shows its whole front

- **WHEN** a rack module inventory is configured and the patch addresses two master jacks
- **THEN** the faceplate renders all sixteen jacks, the LED matrix and the SD button, with the two addressed jacks bright and labelled and the rest dimmed

#### Scenario: Without an inventory the master front is unchanged

- **WHEN** no rack module inventory is configured
- **THEN** the master faceplate renders only the cells the patch addresses, exactly as before

#### Scenario: An inventoried rack paints without a patch

- **WHEN** an inventory is configured and no patch is loaded
- **THEN** the module UI paints the inventory rack with every cell dimmed and publishes no element hit rects

#### Scenario: An inventoried expander shows its jacks

- **WHEN** an inventory lists an R2C that the patch never addresses
- **THEN** its faceplate renders both serial jacks, dimmed and unlabelled

#### Scenario: An unaddressed rotary is tellable by silhouette

- **WHEN** a P2B8 carries one addressed pot and one pot the patch never names, drawn at true size
- **THEN** the unaddressed pot draws the nut, slot and base line without the washer ring, dimmed, so it reads as unused rather than merely unlit

#### Scenario: The unaddressed ring survives where the detail does not

- **WHEN** an unaddressed rotary is drawn at the reduced or centre-dot step, as at the whole-rack fit
- **THEN** it keeps the addressed drawing at the dimmed floor rather than a ringless slot, and stays dimmed so it still reads as unused

#### Scenario: An inventoried master's spare jacks draw socket faces

- **WHEN** an inventoried master draws jacks no patch token addresses
- **THEN** each renders the full socket face dimmed and publishes no hit rect
