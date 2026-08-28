## ADDED Requirements

### Requirement: Graph reflects disabled circuits

In the graph surface, a circuit instance with processing disabled SHALL render its node frame and its incident edges dimmed, overriding any influence highlight on those edges. Hovering a node SHALL keep normal hover styling. The `x` key SHALL toggle processing for the hovered node's circuit instance. Toggling SHALL rebuild the graph and recompute influence, emitting `GraphRebuilt`.

#### Scenario: Disabled node renders dim

- **WHEN** a circuit instance has processing disabled and the graph is rebuilt
- **THEN** that node's frame and all edges incident to it render dim, while enabled nodes and their edges render normally.

#### Scenario: Toggle via x key

- **WHEN** the graph surface is open and a node is hovered
- **THEN** pressing `x` disables (or re-enables) that circuit instance, rebuilds the graph, and shows a status message naming the circuit.

#### Scenario: Influence highlight yields to disabled

- **WHEN** a disabled circuit's edge would otherwise carry an influence highlight
- **THEN** the edge renders dim instead.