# Patch Parsing Specification (delta)

## ADDED Requirements

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
