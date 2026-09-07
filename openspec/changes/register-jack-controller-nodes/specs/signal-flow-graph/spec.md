---
## MODIFIED Requirements

### Requirement: ComfyUI-style rendering
Nodes shall render as rounded frames with a title bar; left-side input ports and right-side output ports; cable edges approximated with box-drawing characters, color-coded by cable type; cluster containers from banner groups surround their circuits.

- Circuit nodes render a rounded frame titled with the circuit name plus the instance index when repeated, colored by the `graph_node_*` circuit tokens.
- Controller nodes render a rounded frame titled with the panel label and chain ordinal (for example `P2B8 #1`), colored by `graph_node_controller`.
- Input-jack and output-jack nodes render a rounded frame titled with the register token (for example `I1`, `O3`), colored by `graph_node_jack_input` and `graph_node_jack_output`.
- Register edges (a circuit reading or writing a hardware register) render with the `graph_edge_register` token under the existing precedence: topology-error red, then diff, then latency, then cable kind.

#### Scenario: Node frame rendering
- **WHEN** the graph contains circuit, controller, input-jack, and output-jack nodes
- **THEN** each node renders as a rounded frame with its kind-specific title and color token.

#### Scenario: Edge color coding
- **WHEN** a cable of type "control" connects two circuits
- **THEN** the edge is rendered in cyan; "audio" in green; "midi" in magenta.

#### Scenario: Register edge color
- **WHEN** a circuit reads or writes a hardware register
- **THEN** the edge renders with the `graph_edge_register` token, and a topology-error or diff classification overrides it.

## ADDED Requirements

### Requirement: Register and hardware nodes
The system SHALL draw hardware registers as graph nodes and register reads/writes as directed edges, in addition to circuits and virtual cables.

- Controller nodes SHALL be created for the controller-declaring sections the parser recognizes (`[p2b8]`, `[b32]`, ...), numbered by chain order.
- Jack nodes SHALL be created on demand for master registers: a circuit writing a register produces an output-jack node, a circuit reading a register consumes an input-jack node.
- A register token SHALL belong to a declared controller when its letter is a controller register letter (`P`, `B`, `E`, `S`, `L`, `R`) and its unit number matches a declared controller ordinal; such tokens resolve to the controller node instead of a jack node.
- Register-edge direction SHALL come from the circuit catalog: a catalog output parameter writes its register references, a catalog input parameter reads them; for a circuit or parameter absent from the catalog, a parameter family of `output` or `led` writes and everything else reads.
- A controller-register letter without a matching declared controller unit SHALL fall back to a jack node (input when read, output when written).
- Jack nodes SHALL be shared across assignments (one node per token) and identical edges SHALL be deduplicated.

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