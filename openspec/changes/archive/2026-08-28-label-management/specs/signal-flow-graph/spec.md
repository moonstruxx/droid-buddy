## ADDED Requirements

### Requirement: Graph node titles use circuit label override
The system SHALL render graph node titles (FULL and FILTERED panes) with the circuit-instance label when present, otherwise the circuit name + instance index, without changing graph physics or layout.

#### Scenario: Graph node override
- **WHEN** `circuits."motorfader:12"` is `T1 Accu` and the node is hovered
- **THEN** both FULL and FILTERED panes show `T1 Accu` as the node title and `hovered_graph_node` targeting is unchanged

