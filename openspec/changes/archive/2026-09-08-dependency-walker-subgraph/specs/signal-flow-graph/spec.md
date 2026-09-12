---
## ADDED Requirements

### Requirement: Upstream dependency subgraph
The system SHALL compute, on request, the transitive set of nodes a chosen root depends on, and SHALL render only that set and the edges among its members.

- The dependency walk SHALL follow incoming edges (reversed) breadth-first from the root, visiting each node once so cycles terminate.
- The walk SHALL stop at controller and input-jack nodes: their incoming edges (LED writes, feedback) are not dependencies. A controller or input-jack root yields only itself.
- On the graph pane, `f` SHALL filter the view to the dependency set of the hovered node, falling back to the shared circuit selection when nothing is hovered, and SHALL be a silent no-op with a status hint when neither exists.
- The root SHALL be frozen when the filter engages; a second `f`, Esc, a patch load, or closing the graph SHALL clear the filter.
- The filtered view SHALL render only the dependency set and the edges with both endpoints in it, solved deterministically as a subset.
- The status bar SHALL report `Dependencies of <label>: N nodes` while the filter is active.
- The filter SHALL be presentation-only: the graph model, the influence traversal, and the patch SHALL remain unchanged.

#### Scenario: Output jack dependencies
- **WHEN** `f` is pressed on a graph containing an output jack fed by two producers and an unrelated circuit
- **THEN** the view renders only the output jack, the two producers, and the edges among them; the unrelated circuit is absent.

#### Scenario: Controller leaf stops the walk
- **WHEN** the dependency walk reaches a controller node via a circuit's LED write
- **THEN** the walk does not follow the controller's incoming edges; the controller is a leaf of the dependency set.

#### Scenario: Cycle terminates
- **WHEN** the graph contains a circular cable dependency
- **THEN** the walk visits each node in the cycle once and terminates.

#### Scenario: Filter clears
- **WHEN** the filter is active and `f` is pressed again, or Esc is pressed, or a patch is loaded
- **THEN** the full graph renders again.

#### Scenario: No root is a no-op
- **WHEN** `f` is pressed with no hovered node and no shared circuit selection
- **THEN** the view is unchanged and the status bar hints that no root is selected.