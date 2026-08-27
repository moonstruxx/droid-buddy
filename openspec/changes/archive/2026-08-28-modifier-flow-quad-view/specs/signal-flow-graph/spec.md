## Purpose

Delta to the existing `signal-flow-graph` spec: add highlight/dim rendering for the FULL graph and compact re-solve semantics for the FILTERED graph. No changes to virtual-cable extraction, banner clusters, convergence layout, or `g g`/`Esc` beyond quad integration.

## ADDED Requirements

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

