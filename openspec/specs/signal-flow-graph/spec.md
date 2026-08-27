# Signal-Flow Graph Specification

## Purpose

A signal-flow node-graph view of DROID patches: circuits as nodes, virtual cables as edges, ComfyUI-style rendering, force-directed convergence layout, and interaction (view-switch, node drag, topology validation).

## Requirements

### Requirement: Virtual-cable extraction
The system SHALL extract virtual cables from parsed circuit sections:

- `output = _NAME` on a circuit creates a cable source.
- Any circuit param `= _NAME` (embedded in arithmetic expressions) references a cable sink.
- Cables in comment lines (`# output = _X`) are ignored.
- A cable source may fan out to any number of sinks (1 → n topology is valid); `n → 1` is invalid and shall be flagged.

#### Scenario: Cable creation from output param
- **WHEN** a circuit section contains `output = _PULSARCLOCK`
- **THEN** a cable source `_PULSARCLOCK` is registered with this circuit as the origin.

#### Scenario: Cable consumption from input param
- **WHEN** a circuit section contains `input = _PULSARCLOCK` (or `frequency = _PULSARCLOCK * 2 - _BASE`)
- **THEN** the cable sink `_PULSARCLOCK` is registered, linking to the source circuit.

#### Scenario: Invalid topology n → 1 flagged
- **WHEN** multiple circuits drive `input = _SINGLE_CLOCK` and no circuit outputs `_SINGLE_CLOCK`
- **THEN** the graph highlights an invalid `n → 1` topology error state.

### Requirement: Banner-range grouping
Comment banners (`# ---- Name ----`) own a range of circuits from themselves to the next banner or end of file. The graph renders each such range as a cluster container with the banner name as the cluster label.

#### Scenario: Banner groups circuits
- **WHEN** a patch has `# ---- Pulsar clock ----` followed by `[clock32in]`, `[threeCV1]`, and later `# ---- Steady clock ----` with `[osc1]`
- **THEN** the graph renders two cluster containers labeled "Pulsar clock" and "Steady clock", each owning its circuits.

### Requirement: Convergence-based layout
The force-directed layout solver shall run bounded iterations until total kinetic energy falls below a threshold, then freeze positions. The solver shall be re-invoked only on patch load or when the user drags a node; no continuous tick or drift occurs.

#### Scenario: Layout converges and freezes
- **WHEN** a patch with 60 circuits is loaded and the graph view opens
- **THEN** the layout runs for a bounded number of iterations until energy < threshold, then freezes; positions do not change on subsequent redraws unless the patch reloads or a node is dragged.

#### Scenario: Re-solve on node drag
- **WHEN** the user drags a node in the graph view
- **THEN** the solver re-invokes (damped, local re-settle) from the new position; other nodes settle quickly without a full global re-run.

### Requirement: ComfyUI-style rendering
Nodes shall render as rounded frames with a title bar; left-side input ports and right-side output ports; cable edges approximated with box-drawing characters, color-coded by cable type; cluster containers from banner groups surround their circuits.

#### Scenario: Node frame rendering
- **WHEN** the graph renders a circuit node
- **THEN** a rounded frame appears with the circuit name in the title bar, left ports on the left edge, right ports on the right edge.

#### Scenario: Edge color coding
- **WHEN** a cable of type "control" connects two circuits
- **THEN** the edge is rendered in cyan; "audio" in green; "midi" in magenta.

### Requirement: View-switch key
The system SHALL provide a `g g` key sequence to open the signal-flow graph view (while in normal mode, no patch loaded or a patch loaded). `Esc` closes the graph and returns focus to the controller panels.

#### Scenario: Open graph with g g
- **WHEN** in normal mode with a patch loaded, the user presses `g` then `g`
- **THEN** the graph view opens alongside (or in place of, depending on layout) the controller panels, showing the signal-flow nodes and edges.

#### Scenario: Close graph with Esc
- **WHEN** the graph view is open and the user presses `Esc`
- **THEN** the graph view closes, focus returns to the controller panels, and any component selection is preserved.

### Requirement: FULL graph highlights influenced path

The system SHALL override per-cable edge color in the FULL graph view when an influence set is present: influenced edges use `graph_edge_highlight` (or `graph_edge_error` if topology validation also applies), uninfluenced edges use `graph_edge_dim` (dimmed). Node frames/borders follow `graph_node_highlight` vs `graph_node_dim` accordingly, preserving pairwise-distinct mono guarantees.

#### Scenario: Influence overrides kind color

- **WHEN** cable `_CLOCK` is of kind `control` (normally cyan) but is uninfluenced
- **THEN** it renders dimmed, not cyan-bright; when influenced it renders with the highlight token, not plain control color.

### Requirement: FILTERED graph is independently solved

The system SHALL independently `solve()` the filtered induced subgraph for its pane (bounded iterations until energy < threshold, then freeze), not reuse FULL graph positions.

#### Scenario: Filtered solve is independent

- **WHEN** the FILTERED graph has N nodes
- **THEN** its positions differ from the subset of FULL positions and it converges inside its own pane's bounding box.
