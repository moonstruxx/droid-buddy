# Patch Parsing Specification

## Purpose

Parse DROID `.ini` patch files to extract hardware component definitions (buttons, knobs, CV I/O, encoders, LEDs, switches) and build a typed `Patch` struct that the TUI can render and interact with.

## Requirements

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

### Requirement: Preserve raw source lines
The system SHALL retain the verbatim text of every line of the loaded `.ini` file (including comments and blank lines) as part of the parsed patch, without changing existing parse results.

#### Scenario: Raw text round-trips
- **WHEN** a patch file is loaded
- **THEN** concatenating the retained lines reproduces the input file's lines exactly

#### Scenario: Existing parsing unchanged
- **WHEN** the fixture `fixtures/arpeggio1.ini` is parsed
- **THEN** components, sections, and shift groups parse identically to before this change

### Requirement: Record source spans for sections and tokens
The system SHALL record, for each parsed section and each extracted hardware-token reference, the line number (and column range) where it appears in the raw text.

#### Scenario: Section span recorded
- **WHEN** `[button]` starts on line 4 of the file
- **THEN** that section records line 4 as its header position

#### Scenario: Token span recorded
- **WHEN** `led = L1.2` appears on line 7 with the token starting at column 7
- **THEN** token `L1.2` records line 7 and its exact column range

#### Scenario: Boundary-aware extraction preserved
- **WHEN** a value contains `_ENV1_DECAY_POT_ABSBIPOLAR * -1`
- **THEN** no span is recorded for internal variables (they are not hardware tokens)

### Requirement: Provide occurrence lookup
The system SHALL expose a lookup from hardware token to its occurrences in source reading order, built once at load time from the recorded spans.

#### Scenario: Lookup returns ordered occurrences
- **WHEN** the lookup is queried for a token referenced by multiple circuits
- **THEN** occurrences are returned ordered top-to-bottom by line number

#### Scenario: Unknown token yields empty result
- **WHEN** the lookup is queried for a token absent from the patch
- **THEN** an empty occurrence list is returned without error

### Requirement: Resolve select/selectat modifier relationships
The system SHALL resolve, at load time, which circuit assignments react to which hardware components for `select`-family inputs: `select = X` reacts when `X` is positive; `select = X` with `selectat = N` reacts when `X == N`; `X` may be a direct hardware token or an internal cable/variable producer resolved transitively. Resolution SHALL terminate on cyclic definitions.

#### Scenario: Boolean select resolution
- **WHEN** a section contains `select = B1.2`
- **THEN** that assignment is resolved as reacting to component `B1.2`'s pressed state

#### Scenario: Exact-value selectat resolution
- **WHEN** a section contains `select = P1.1` and `selectat = 0.5`
- **THEN** that assignment is resolved as reacting to `P1.1` exactly at value 0.5

#### Scenario: Transitive producer resolution
- **WHEN** an internal variable derives from hardware token `B1.3` and another section selects on that variable
- **THEN** the select assignment is resolved as reacting to `B1.3`

#### Scenario: Cyclic definitions terminate
- **WHEN** internal variables reference each other in a cycle
- **THEN** resolution completes without hanging, resolving what is reachable

### Requirement: Record LED association
When a section assigns `led = L.N`, the parser SHALL link that LED token to the element defined in the same section (e.g. `b = B1.1`), exposing the association on the parsed patch's component as an optional LED reference.

#### Scenario: Button with LED
- **WHEN** a `[button]` section contains `b = B1.1` and `led = L1.1`
- **THEN** the parsed button component carries the association `led: Some("L1.1")` alongside its id `B1.1`

#### Scenario: Section without led
- **WHEN** a section defines an element but no `led =` assignment
- **THEN** the parsed component has no LED association (`led: None`)

#### Scenario: Existing parse unchanged
- **WHEN** a patch contains no `led =` assignments at all
- **THEN** every component parses with `led: None` — no behavioral change