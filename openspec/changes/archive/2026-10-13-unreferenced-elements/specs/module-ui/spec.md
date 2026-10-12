## MODIFIED Requirements

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

### Requirement: E4 rings follow the LED registers

Each E4 encoder's ring SHALL render the 32 LED registers assigned to that encoder (L1.1–L1.32 for the first encoder, L1.33–L1.64 for the second, and so on), each segment showing its register's brightness and colour. The ring SHALL NOT be derived from the encoder's value. Ring segments the patch does not reference SHALL render dimmed.

#### Scenario: Patch lights part of a ring

- **WHEN** a patch lights L1.1–L1.8 and leaves L1.9–L1.32 dark
- **THEN** the first encoder's ring shows eight lit segments and 24 dark segments

#### Scenario: Bare E4 shows full dimmed rings

- **WHEN** a patch declares a bare `[e4]` (synthesizing 4 encoders, 4 buttons, 4 LEDs)
- **THEN** all 128 ring segments render, the referenced ones lit and the rest dimmed and unlabelled
