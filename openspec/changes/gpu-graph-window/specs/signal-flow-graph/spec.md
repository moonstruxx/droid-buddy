# signal-flow-graph

## ADDED Requirements

### Requirement: GPU window surface

When the `gui` feature is enabled, the signal-flow graph SHALL also be renderable in a GPU-accelerated window that shows the same graph model, solver positions, and camera as the terminal tile. Every existing graph interaction (node drag with re-settle and `NodeMoved`, hover, `x` disable, `p` pin, `e` label overlay, diff and latency coloring, topology-error highlighting) SHALL behave identically in the window.

#### Scenario: Graph renders in the window

- **WHEN** the `gui` feature is enabled and the user opens the GPU window
- **THEN** the graph renders with the same nodes, edges, clusters, and colors as the terminal tile

#### Scenario: Interactions behave identically

- **WHEN** the user drags a node in the GPU window
- **THEN** the node moves with the cursor, the local neighborhood re-settles, and `NodeMoved` is emitted, exactly as on the terminal path

### Requirement: Circuit selection shared across surfaces

Selecting a circuit node in any graph surface (GPU window or terminal tile) SHALL set a shared selection that the panels and source viewer reflect: the source viewer jumps to the circuit's section and the panels highlight the associated hardware.

#### Scenario: Selection from terminal tile

- **WHEN** the user selects a circuit node in the terminal graph tile
- **THEN** the source viewer jumps to that circuit's section and the panels highlight the associated hardware