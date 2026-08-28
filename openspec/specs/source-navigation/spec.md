# Source Navigation Specification

## Purpose

Give users line-accurate navigation from hardware components to their places in the raw `.ini` patch: a retained source text with per-token spans, an occurrence index, selection-driven jumps, occurrence stepping, and modifier-aware highlighting of every source fragment affected by the selected component.

## Requirements

### Requirement: Line-accurate source model
The system SHALL retain the patch's raw `.ini` text and record a source span (line number plus column range) for every parsed section header and every hardware-token reference found in circuit values.

#### Scenario: Raw lines preserved
- **WHEN** a patch file is loaded
- **THEN** the source text is available verbatim, including comments and blank lines

#### Scenario: Token spans recorded
- **WHEN** a `[button]` section contains `button = B1.1`
- **THEN** the token `B1.1` has a recorded span pointing at its exact position in the source text

### Requirement: Hardware token occurrence index
The system SHALL maintain, per loaded patch, an index mapping each hardware token to its occurrences in source reading order (top-to-bottom, left-to-right).

#### Scenario: Occurrences in reading order
- **WHEN** token `B1.1` is referenced by three different circuits
- **THEN** the index lists three occurrences ordered by their position in the file

#### Scenario: Internal variables excluded
- **WHEN** a circuit contains `input = _ENV1_DECAY_POT_ABSBIPOLAR * -1`
- **THEN** `_ENV1_DECAY_POT_ABSBIPOLAR` produces no occurrence (it is not a hardware token)

### Requirement: Selection-driven source jump
The system SHALL scroll the source pane to the first occurrence of the selected component when a component is selected. Selecting a different component SHALL jump to that component's first occurrence. Clearing the selection SHALL NOT move the source position.

#### Scenario: First selection jumps to first occurrence
- **WHEN** the user selects a component referenced in multiple circuits
- **THEN** the source pane scrolls to the component's first occurrence

#### Scenario: Replacement selection jumps again
- **WHEN** a component is selected and the user selects a different component
- **THEN** the source pane jumps to the new component's first occurrence

#### Scenario: Deselection keeps position
- **WHEN** a component is selected and the user clears the selection
- **THEN** the source scroll position is unchanged

### Requirement: Initial viewer position
When the source pane opens with a selection active, the system SHALL position it at the selected component's first occurrence; with no selection it SHALL position at the beginning of the file.

#### Scenario: Opens at beginning of file
- **WHEN** the source pane opens and no component is selected
- **THEN** the source view starts at the first line of the file

#### Scenario: Opens at selected component
- **WHEN** the source pane opens while a component is selected
- **THEN** the source view starts at that component's first occurrence

### Requirement: Occurrence navigation
While the source pane is focused, Up SHALL move to the previous occurrence, Down to the next occurrence, Home to the first occurrence, and End to the last occurrence of the selected component, scrolling the source to bring the target occurrence into view.

#### Scenario: Next occurrence
- **WHEN** a component with three occurrences is selected and the cursor is on the first occurrence
- **THEN** pressing Down moves to the second occurrence and scrolls it into view

#### Scenario: Navigation bounds saturate
- **WHEN** the cursor is on the first occurrence and the user presses Up
- **THEN** the cursor stays on the first occurrence (saturating, no wrap)

#### Scenario: First and last occurrence
- **WHEN** a component with occurrences is selected
- **THEN** Home moves to the first occurrence and End moves to the last occurrence

#### Scenario: No selection
- **WHEN** no component is selected and the user presses Up/Down/Home/End in the source pane
- **THEN** nothing moves (there is no occurrence sequence to traverse)

### Requirement: Modifier relationship resolution
The system SHALL resolve which source fragments react to a selected component by analyzing `select`/`selectat` assignments: `select = X` activates when `X` is positive; `select = X` together with `selectat = N` activates when `X == N`; `X` may be a direct hardware token or produced by internal cables/variables resolved transitively. Traversal SHALL be cycle-safe.

#### Scenario: Direct hardware source
- **WHEN** a circuit contains `select = B1.2` and the user selects component `B1.2`
- **THEN** that `select` line is highlighted as affected

#### Scenario: Exact-value activation
- **WHEN** a circuit contains `select = P1.1` and `selectat = 0.5` and knob `P1.1` holds value 0.5
- **THEN** that `select` line is highlighted as affected

#### Scenario: Value mismatch not highlighted
- **WHEN** a circuit contains `select = P1.1` and `selectat = 0.5` and knob `P1.1` holds value 0.2
- **THEN** that `select` line is not highlighted

#### Scenario: Transitive internal producer
- **WHEN** an internal variable is computed from `B1.3` and a distant circuit contains `select = <that variable>`
- **THEN** the distant `select` line is highlighted when `B1.3` is selected

#### Scenario: Cyclic patch definitions
- **WHEN** internal variables form a definition cycle
- **THEN** resolution terminates and highlights what was resolved without hanging

### Requirement: Modifier highlight rendering scope
Selecting a component SHALL highlight every source fragment resolved as affected by it; clearing the selection SHALL remove all highlights. Highlighting is advisory only — it never mutates component or patch state.

#### Scenario: Highlights follow selection
- **WHEN** the user selects a different component
- **THEN** highlights switch to the fragments affected by the newly selected component

#### Scenario: Highlights cleared on deselection
- **WHEN** the selection is cleared
- **THEN** no source fragments remain highlighted

### Requirement: Source section header circuit label override
The system SHALL render a circuit section header with the circuit-instance label from `LabelStore` when present, otherwise the raw `[circuit]` header, without altering scroll or occurrence indexing.

#### Scenario: Header override
- **WHEN** `circuits."motorfader:12"` is set
- **THEN** the source pane header for that section shows the circuit label while `occurrence_index` still maps to the original span
