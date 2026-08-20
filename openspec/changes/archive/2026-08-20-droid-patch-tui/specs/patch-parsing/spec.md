## Purpose

Parse DROID `.ini` patch files to extract hardware component definitions (buttons, knobs, CV I/O, encoders, LEDs, switches) and build a typed `Patch` struct that the TUI can render and interact with.

## ADDED Requirements

### Requirement: Parse .ini patch files
The system SHALL parse DROID `.ini` patch files, extracting all circuit sections (e.g., `[button]`, `[encoder]`, `[p2b8]`, `[faderbank]`, `[notebuttons]`, `[pot]`, `[lfo]`, `[arpeggio]`) and their key-value pairs.

#### Scenario: Valid patch file parsed
- **WHEN** a well-formed DROID `.ini` file is loaded
- **THEN** all circuit sections and their key-value pairs are extracted without error

#### Scenario: Empty or invalid file
- **WHEN** an empty or malformed `.ini` file is loaded
- **THEN** the system returns a descriptive error and does not crash

### Requirement: Extract hardware tokens
The system SHALL identify and extract hardware token references from circuit section values, including: `B` (buttons, e.g., `B1.1`), `L` (LEDs, e.g., `L1.2`), `P` (pots/knobs, e.g., `P1.1`), `O` (CV outputs, e.g., `O1`), `I` (CV inputs, e.g., `I1`), `E` (encoders, e.g., `E1.1`), `S` (switches, e.g., `S1.3`).

#### Scenario: Token extraction from button section
- **WHEN** a `[button]` section contains `button = B1.1` and `led = L1.1`
- **THEN** tokens `B1.1` (Button) and `L1.1` (Led) are extracted

#### Scenario: Token extraction from mixed expressions
- **WHEN** a circuit section contains `input = _ENV1_DECAY_POT_ABSBIPOLAR * -1 + _DECAY_MIN`
- **THEN** no hardware tokens are extracted (internal variables are not hardware tokens)

#### Scenario: Token extraction from p2b8 section
- **WHEN** a `[p2b8]` section is encountered (which implicitly defines B1.1-B1.8, L1.1-L1.8, P1.1-P1.2)
- **THEN** all 18 hardware tokens for the P2B8 controller are extracted

### Requirement: Map tokens to typed components
The system SHALL map each extracted hardware token to a typed `HwComponent` with the correct `ComponentKind` (Button, Led, Knob, CvIn, CvOut, Encoder, Switch), a human-readable label derived from the circuit context, and an initial `ComponentState`.

#### Scenario: Button token mapped
- **WHEN** token `B1.1` is extracted from a `[button]` section
- **THEN** a `HwComponent` with `kind: Button`, `id: "B1.1"`, and `label` derived from the circuit's purpose is created

#### Scenario: CV output token mapped
- **WHEN** token `O1` is extracted from a circuit section
- **THEN** a `HwComponent` with `kind: CvOut` and `id: "O1"` is created

### Requirement: Derive patch name from filename
The system SHALL derive the patch name from the `.ini` filename (without extension) and store it in the `Patch.name` field.

#### Scenario: Patch name from filename
- **WHEN** loading `arpeggio1.ini`
- **THEN** `Patch.name` is set to `"arpeggio1"`

### Requirement: Identify shift groups from circuit context
The system SHALL analyze circuit sections to identify which hardware components belong to which shift group, based on the DROID patch's button/LED grouping patterns (e.g., B1.1-B1.8 on a P2B8 typically share shift behavior).

#### Scenario: P2B8 components grouped
- **WHEN** a `[p2b8]` section defines buttons B1.1-B1.8
- **THEN** those components are associated with a common shift group
