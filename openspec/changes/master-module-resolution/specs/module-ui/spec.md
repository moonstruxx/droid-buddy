# Spec Delta

## MODIFIED Requirements

### Requirement: Master faceplate

Exactly one master faceplate SHALL render — the master model resolved for the patch, MASTER or MASTER18 — in its rack position, with that model's jacks, LEDs and button, replacing any separate CV I/O group. Input and output jacks SHALL be distinguishable. The module UI's pane title SHALL name the resolved model, so the one thing the faceplate cannot show by itself — which of the two masters is drawn — is always readable. An element the patch addresses that the resolved master has no cell for SHALL draw no cell: the view SHALL NOT invent a jack, and the corresponding `master_conflict` finding is what explains the absence.

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
