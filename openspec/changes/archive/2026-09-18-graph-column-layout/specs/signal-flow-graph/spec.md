## ADDED Requirements

### Requirement: Column layout arrangement

The graph layout SHALL provide a deterministic column arrangement as its default. Each node SHALL be assigned a column from topological depth (the capped Bellman-Ford depth, dense-normalized to `0..N-1`), and nodes SHALL be placed width-aware within their column: the column's width from its widest node, column blocks centered horizontally, nodes stacked vertically with spacing so nodes in a column never overlap, all coordinates snapped to the layout grid. Within-column ordering SHALL be strict slot order by default and SHALL offer a configurable barycenter crossing-minimization option. Controller and jack nodes SHALL sit in fixed outer columns on the column arrangement, with circuit columns between them. The arrangement SHALL be deterministic: same patch, same machine, same layout. The user SHALL be able to toggle to the force-directed arrangement; the column arrangement is the default.

#### Scenario: Nodes arrange into non-overlapping columns

- **WHEN** a patch with 60 circuits is loaded and the graph view opens
- **THEN** nodes arrange into dense-normalized columns aligned left to right, no two nodes in a column overlap, and coordinates sit on the layout grid.

#### Scenario: Column layout is deterministic

- **WHEN** the same patch is loaded twice on the same machine
- **THEN** both runs produce identical node positions.

#### Scenario: Controller and jack nodes sit outside the circuit columns

- **WHEN** the graph view opens on the column arrangement
- **THEN** controller, input-jack, and output-jack nodes render in fixed outer columns with the circuit columns between them.

#### Scenario: Force layout reachable by toggle

- **WHEN** the user toggles from the column arrangement to the force arrangement
- **THEN** the graph re-solves with the force-directed layout and the choice persists for the session.

### Requirement: Configurable within-column ordering

The column arrangement SHALL order nodes within a column either by strict slot order (the default) or by barycenter crossing-minimization sweeps, selected by user configuration. The choice SHALL not affect which columns nodes land in, only their vertical order within a column.

#### Scenario: Strict slot order default

- **WHEN** no ordering preference is configured
- **THEN** nodes stack within each column in strict slot order.

#### Scenario: Barycenter ordering configured

- **WHEN** the ordering preference is set to barycenter crossing-minimization
- **THEN** nodes within each column order by the crossing-minimization sweeps while column assignment stays unchanged.

## MODIFIED Requirements

### Requirement: Convergence-based layout

The force-directed layout SHALL bias the layout toward a single dominant left→right axis: initial positions seed by topological layer (depth → x, within-layer order → y) and the solver converges to a horizontal chain. Cable spring force SHALL dominate the repulsion force so connected circuits cohere into a readable flow. The solver SHALL run bounded iterations until total kinetic energy falls below a threshold, then freeze positions. The solver SHALL be re-invoked only on patch load or when the user drags a node; no continuous tick or drift occurs. Pinned nodes (including the tip, the first circuit in `.ini` order) SHALL act as fixed anchors the solver never moves. This arrangement SHALL be the non-default option, reachable by the layout toggle; the column arrangement is the default.

#### Scenario: Layout converges and freezes

- **WHEN** a patch with 60 circuits is loaded, the graph view opens, and the force arrangement is active
- **THEN** the layout runs for a bounded number of iterations until energy < threshold, then freezes; positions do not change on subsequent redraws unless the patch reloads or a node is dragged.

#### Scenario: Re-solve on node drag

- **WHEN** the user drags a node in the graph view while the force arrangement is active
- **THEN** the solver re-invokes (damped, local re-settle) from the new position; other nodes settle quickly without a full global re-run, and pinned nodes stay put.

#### Scenario: Layout converges to a horizontal chain

- **WHEN** a patch with 60 circuits is loaded and the graph view opens on a wide terminal with the force arrangement active
- **THEN** the layout converges along a single left→right axis and fills the canvas width, rather than stacking vertically.

#### Scenario: Cable springs pull circuits together

- **WHEN** two circuits are connected by a cable in the force arrangement
- **THEN** they sit nearer each other than unconnected circuits, so edges read as springs.

#### Scenario: Tip stays anchored

- **WHEN** the graph converges in the force arrangement
- **THEN** the first circuit (`.ini` order) remains at the left and does not drift during the solve or on subsequent redraws.

### Requirement: Cable tension keys

`Alt+[`/`Alt+]` SHALL lower/raise the graph's cable tension (solver spring stiffness) when the graph pane has focus and the force arrangement is active, re-solving the layout live. On the column arrangement the keys SHALL have no effect.

#### Scenario: Adjust cable tension

- **WHEN** the graph pane is focused, the force arrangement is active, and the user presses `Alt+]`
- **THEN** the cable tension increases by 0.05 and the layout re-solves

#### Scenario: Tension keys inert on the column arrangement

- **WHEN** the column arrangement is active and the user presses `Alt+]`
- **THEN** the layout does not change and no status is shown.

### Requirement: Manual pin and drag-to-place

The system SHALL let the user pin circuit nodes so manual placements survive the solver. Pinned nodes SHALL be fixed anchors the solver never moves. The `p` key SHALL toggle pin/unpin on the hovered graph node, and dragging a node SHALL place it as a fixed anchor (auto-pin at the dropped position). The first circuit in `.ini` file order SHALL be pinned by default as the graph's tip. Unpinning a node SHALL release it to re-flow. On the column arrangement, pinned nodes SHALL act as fixed-position anchors when the arrangement is recomputed.

#### Scenario: Tip is pinned by default

- **WHEN** a patch's graph opens
- **THEN** the first circuit in `.ini` order sits as a fixed left anchor and does not drift.

#### Scenario: Pin then place a node

- **WHEN** the user pins a hovered node with `p` and then drags it to a new position
- **THEN** the node stays where it was dropped, and subsequent solves do not move it.

#### Scenario: Drag auto-pins an unpinned node

- **WHEN** the user drags an unpinned node
- **THEN** it is pinned at the dropped position and remains there rather than snapping back to spring equilibrium.

#### Scenario: Unpin re-flows

- **WHEN** the user presses `p` on a pinned node
- **THEN** the node is released and the solver re-flows it into the layout.

#### Scenario: Pinned nodes anchor the column arrangement

- **WHEN** the column arrangement recomputes while a node is pinned
- **THEN** the pinned node keeps its fixed position and the remaining nodes arrange around it.