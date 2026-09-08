---
## MODIFIED Requirements

### Requirement: Register and hardware nodes
The system SHALL draw hardware registers as graph nodes and register reads/writes as directed edges, in addition to circuits and virtual cables.

- Controller nodes SHALL be created for the controller-declaring sections the parser recognizes (`[p2b8]`, `[b32]`, ...), numbered by chain order.
- Jack nodes SHALL be created on demand for master registers: a circuit writing a register produces an output-jack node, a circuit reading a register consumes an input-jack node.
- A register token SHALL belong to a declared controller when its letter is a controller register letter (`P`, `B`, `E`, `S`, `L`, `R`) and its unit number matches a declared controller ordinal; such tokens resolve to the controller node instead of a jack node.
- Register-edge direction SHALL come from the circuit catalog: a catalog output parameter writes its register references, a catalog input parameter reads them; for a circuit or parameter absent from the catalog, a parameter family of `output` or `led` writes and everything else reads.
- A controller-register letter without a matching declared controller unit SHALL fall back to a jack node (input when read, output when written).
- Jack nodes SHALL be shared across assignments (one node per token) and identical edges SHALL be deduplicated.
- When an assumed select state is set, a circuit classified NotSelected SHALL drop its controller register edges while keeping its cable and jack edges; with `hide_unselected` it SHALL be dropped from the graph entirely.

#### Scenario: Button read edge
- **WHEN** a button circuit contains `button = B1.1` and a controller is declared at ordinal 1
- **THEN** an edge from the controller node to the circuit renders, labeled `B1.1`.

#### Scenario: LED write edge
- **WHEN** a circuit contains `led = L1.1` and a controller is declared at ordinal 1
- **THEN** an edge from the circuit to the controller node renders, labeled `L1.1`.

#### Scenario: CV input jack
- **WHEN** a circuit reads `I1` in a catalog input parameter
- **THEN** an input-jack node `I1` is created (shared across circuits) and an edge from it to the circuit renders.

#### Scenario: CV output jack
- **WHEN** a circuit writes `O3` in a catalog output parameter
- **THEN** an output-jack node `O3` is created and an edge from the circuit to it renders.

#### Scenario: Unmatched controller register
- **WHEN** a circuit references `P3.1` but no controller is declared at ordinal 3
- **THEN** the token resolves to a jack node, not a controller node.

#### Scenario: Unknown circuit register convention
- **WHEN** a circuit absent from the catalog contains `led = L1.1` or `output = O1`
- **THEN** the assignment is treated as a write (circuit to target), matching the cable-direction fallback.

#### Scenario: Register edge deduplication
- **WHEN** two assignments in the same circuit write the same register with the same label
- **THEN** only one edge renders between the circuit and the target node.

#### Scenario: NotSelected circuit keeps signal flow
- **WHEN** a circuit's select expression evaluates to a value different from the assumed state
- **THEN** the circuit drops its controller register edges but keeps its cable and jack edges, and its node renders dim.

## ADDED Requirements

### Requirement: Select-state filtering
The system SHALL evaluate Droid select expressions against an assumed patch state and rebuild the graph so only the circuits that own the hardware keep their controller connections.

- The system SHALL evaluate expressions left to right with no operator precedence and no parentheses; a numeric literal with a `V` suffix SHALL be divided by 10 (`1V` = 0.1); an expression referencing a signal without an assumed value or using unrecognized syntax SHALL evaluate to unknown.
- The system SHALL discover every signal referenced by a `select` input, ranked by how many circuits select on it, with candidate values inferred from `selectat` literals, button-group `valueN` outputs, and the default `0`/`1` pair.
- A circuit SHALL be Selected when it has no `select` entry or its select expression equals the assumed value; NotSelected when it evaluates to another value; Unknown when the expression cannot be evaluated.
- The user SHALL open the select-state menu with `g s`, navigate signals with `j`/`k`, cycle candidate values with `[`/`]`, and close it with Esc, which clears the assumed state and restores the unassumed graph.
- The status bar SHALL report the selection summary while a state is assumed.
- The assumed state SHALL reset on patch load.

#### Scenario: Shared controller selection
- **WHEN** two circuits select on `S7.1` with `selectat` values `0` and `1` and the menu assumes `S7.1 = 0`
- **THEN** the first circuit keeps its controller edges and the second drops them, keeps its cables and jacks, and renders dim.

#### Scenario: Unknown expression keeps edges
- **WHEN** a select expression references a signal with no assumed value
- **THEN** the circuit is Unknown and keeps all its edges, and the menu makes no claim about it.

#### Scenario: Candidate inference from selectat
- **WHEN** a circuit's select is the bare signal `S7.1` and its `selectat` is `0.5`
- **THEN** `0.5` appears among the candidate values offered for `S7.1`.

#### Scenario: State resets on load
- **WHEN** a new patch is loaded while a select state is assumed
- **THEN** the assumed state is cleared and the graph renders unassumed.