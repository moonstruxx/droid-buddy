# Spec Delta

## MODIFIED Requirements

### Requirement: Unused elements are drawn

The module UI SHALL render every hardware element of a declared controller, including elements the patch does not reference. Unreferenced elements SHALL render dimmed and without a label. Unreferenced elements SHALL NOT publish hit rects and SHALL NOT be reachable by keyboard navigation, selection, the label editor, or modifier latching. An unreferenced LED on a fold-rule controller (P2B8, B32, M4) SHALL NOT render as a standalone cell (the fold rule covers its face); all other unreferenced LEDs SHALL render dimmed standalone.

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

### Requirement: Master faceplate

Exactly one master faceplate SHALL render — the master model resolved for the patch, MASTER or MASTER18 — in its rack position, with that model's jacks, LEDs and button, replacing any separate CV I/O group. Input and output jacks SHALL be distinguishable. The module UI's pane title SHALL name the resolved model, so the one thing the faceplate cannot show by itself — which of the two masters is drawn — is always readable. An element the patch addresses that the resolved master has no cell for SHALL draw no cell: the view SHALL NOT invent a jack, and the corresponding `master_conflict` finding is what explains the absence.

On an inventory-driven chain the faceplate SHALL render the model's complete field, including MASTER18's gate column with `I1`, `I2` and `G1..G4`. When no patch is loaded but an inventory is configured, the faceplate SHALL render that field from the default master model.

#### Scenario: Master in the rack

- **WHEN** a patch uses master inputs and outputs
- **THEN** a master faceplate renders at the start of the rack with its 4×4 jack grid, its LEDs beside the jacks, and no separate CV I/O panel

#### Scenario: MASTER18 renders as MASTER18

- **WHEN** a patch declares `master=18`
- **THEN** the faceplate is the MASTER18 face (6 HP, `O1..O8` and its gate jacks) with no LED matrix, and the pane title names MASTER18

#### Scenario: The inferred model is named too

- **WHEN** a patch declares no master and resolves to MASTER
- **THEN** the pane title names MASTER

#### Scenario: An element the model cannot host is not drawn

- **WHEN** a patch declaring `master=18` addresses `I3`
- **THEN** no cell is drawn for that token on the MASTER18 faceplate, and the `master_conflict` finding names the jack

#### Scenario: MASTER18's gate column renders on an inventoried rack

- **WHEN** a rack module inventory is configured and the patch declares `master=18`
- **THEN** the faceplate renders `O1..O8`, the `I1`/`I2` logic inputs, the `G1..G4` gate outputs and the SD button, with the addressed ones bright
