# Spec Delta

## MODIFIED Requirements

### Requirement: Convergence-based layout

The force-directed layout SHALL bias the layout toward a single dominant left→right axis: initial positions seed by topological layer (x from the layer index, y from the within-layer order) and the solver converges to a horizontal chain. The layer index used for the seed SHALL be the **dense-normalized rank** of the node's topological depth, not the raw depth value, so that the seed's horizontal extent is bounded by the number of distinct layers and stays within the integrator's travel budget even when the graph is cyclic. Cable spring force SHALL dominate the repulsion force so connected circuits cohere into a readable flow. The solver SHALL run bounded iterations until total kinetic energy falls below a threshold, then freeze positions. The solver SHALL be re-invoked only on patch load or when the user drags a node; no continuous tick or drift occurs. Pinned nodes (including the tip, the first circuit in `.ini` order) SHALL act as fixed anchors the solver never moves. This arrangement SHALL be the non-default option, reachable by the layout toggle; the column arrangement is the default.

The dense-normalized layer index SHALL be derived deterministically by sorting and deduplicating the depth values, so layout determinism is preserved: same patch + same machine + same tension → identical positions. Nodes whose depth values already form a contiguous `0..N-1` sequence SHALL be seeded exactly as before, keeping such layouts byte-identical.

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

#### Scenario: Cyclic patch seeds within the integrator's reach

- **WHEN** a deeply cyclic patch (feedback cables making the raw topological depth far exceed the node count) is arranged with the force solver
- **THEN** the seed's horizontal extent is bounded by `(distinct layer count − 1) × HORIZONTAL_SPACING`
- **AND** the solved layout's bounding box stays proportional to patch size instead of exploding with the cycle length

#### Scenario: Contiguous-depth layout is unchanged

- **WHEN** a graph's topological depth values already form a contiguous sequence starting at 0
- **THEN** the dense-normalized rank equals the raw depth for every node
- **AND** the seed positions and the solved positions are byte-identical to the previous behaviour

#### Scenario: Force-mode determinism holds

- **WHEN** the same patch is solved twice in force mode at the same tension
- **THEN** both solves return identical positions
