## ADDED Requirements

### Requirement: Per-modifier hue for influenced graph elements

When a modifier is active, the system SHALL recolor every edge (cable) and node (circuit) in that modifier's structural influence set with that modifier's stable hue in the full graph view, and the filtered graph's induced subgraph SHALL be tinted likewise. Additive latches SHALL union hues (each edge/node keeps its source modifier's hue; most-recent wins on overlap shared by two modifiers). The hue overrides the default `CableKind`-derived edge color but yields to topology-error red (`graph_edge_error`) when a cable is invalid.

#### Scenario: Edge hue

- **WHEN** `B1.1` is held and its influence includes `_TRIG` → `arpeggio`
- **THEN** the `_TRIG` edge and `arpeggio` node render in `B1.1`'s hue.

#### Scenario: Additive

- **WHEN** `B1.1` and `B1.2` are latched
- **THEN** edges/nodes from each show their respective hue.

#### Scenario: Error precedence

- **WHEN** an influenced cable is also a topology error (`n→1`)
- **THEN** it renders in `graph_edge_error` red, not the modifier hue.
