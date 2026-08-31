# Signal-Flow Graph — delta

## MODIFIED Requirements

### Requirement: Edge color from cable kind
The system SHALL color a graph edge by the declared `cable_kind` of the producing circuit when present, falling back to name-substring inference when absent. This preserves the existing classification of all embedded circuits byte-for-byte while letting plugin circuits declare their kind.

#### Scenario: Plugin circuit with declared kind
- **WHEN** a graph edge is produced by a plugin circuit that declares `cable_kind`
- **THEN** the edge renders with the token for that kind (control/audio/midi), regardless of the circuit's name substrings

#### Scenario: Embedded circuit unchanged
- **WHEN** a graph edge is produced by an embedded circuit with no declared kind
- **THEN** the edge renders exactly as before this change (substring inference)

### Requirement: Node color for plugin circuits
The system SHALL color a graph node for a plugin circuit using the circuit's declared `color` when present, falling back to substring inference otherwise.

#### Scenario: Plugin circuit with declared color
- **WHEN** a graph node is a plugin circuit that declares a `color` token
- **THEN** the node renders with that token

#### Scenario: Plugin circuit without declared color
- **WHEN** a graph node is a plugin circuit with no declared `color`
- **THEN** the node renders via the existing name-inference path, with no error