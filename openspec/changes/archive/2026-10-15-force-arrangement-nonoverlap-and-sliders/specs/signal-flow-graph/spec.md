# Spec Delta

## MODIFIED Requirements

### Requirement: Convergence-based layout

The force-directed layout SHALL bias the layout toward a single dominant left→right axis: initial positions seed by topological layer (x from the layer index, y from the within-layer order) and the solver converges to a horizontal chain. The layer index used for the seed SHALL be the **dense-normalized rank** of the node's topological depth, not the raw depth value, so that the seed's horizontal extent is bounded by the number of distinct layers and stays within the integrator's travel budget even when the graph is cyclic. Cable spring force SHALL dominate the repulsion force so connected circuits cohere into a readable flow. The solver SHALL run bounded iterations until total kinetic energy falls below a threshold, then freeze positions. The solver SHALL be re-invoked only on patch load or when the user drags a node; no continuous tick or drift occurs. Pinned nodes (including the tip, the first circuit in `.ini` order) SHALL act as fixed anchors the solver never moves. This arrangement SHALL be the non-default option, reachable by the layout toggle; the column arrangement is the default.

The dense-normalized layer index SHALL be derived deterministically by sorting and deduplicating the depth values, so layout determinism is preserved: same patch + same machine + same knob tuple → identical positions. The seed of a graph whose depth values already form a contiguous `0..N-1` sequence SHALL be identical to the previous behaviour.

Repulsion SHALL be **size-aware**: a node body is its `(estimated width, NODE_WORLD_H)` extent — the same extent the renderer draws and hit-tests — and two bodies whose centre distance is below their required separation `(extent_i + extent_j) / 2` SHALL repel with a bounded linear penetration ramp that reaches the maximum per-component displacement inside one repulsion radius, while pairs beyond their required separation SHALL keep the retained inverse-square falloff up to its own radius. The repulsion neighbour grid SHALL be sized so that any pair within required separation is considered, and the repulsion magnitude SHALL scale with the repulsion knob.

**Non-overlap SHALL be a post-condition** of every solver path that produces positions: the force solve, the drag re-settle, and the column arrangement's pinned placement. The pass SHALL correct an intersecting pair by moving both bodies unless one is immovable — a pinned node, or a node outside a drag's re-settle neighbourhood — in which case the movable partner takes the whole correction. After the post-condition no two bodies SHALL intersect **unless an active body is caged between immovable bodies**: a body whose separation from every other body along both axes is blocked by pinned or out-of-neighbourhood neighbours keeps its intersection no matter how many passes run (measured on the 511-body anchor: six such pairs survive 8192 sweeps, so the residue is a geometric cage, not a pass budget). That exception is deliberately bounded to user-fixed and out-of-neighbourhood bodies — the solver's own layouts and every fully-movable pair are clear. Pinned nodes SHALL be honored exactly unless two pinned bodies themselves intersect, in which case the no-overlap invariant takes precedence and the intersecting bodies are separated.

The solver SHALL take its physics as an explicit knob tuple (cable tension, slice force, section force, repulsion gain, cohesion), with documented defaults reproducing the previous sections and column behaviour. Cohesion SHALL scale with the cohesion knob.

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

#### Scenario: Contiguous-depth seed is unchanged

- **WHEN** a graph's topological depth values already form a contiguous sequence starting at 0
- **THEN** the dense-normalized rank equals the raw depth for every node
- **AND** the seed positions are byte-identical to the previous behaviour

#### Scenario: Force-mode determinism holds

- **WHEN** the same patch is solved twice in force mode with the same knob tuple
- **THEN** both solves return identical positions

#### Scenario: No overlapping bodies in the force arrangement

- **WHEN** any patch is solved with the force arrangement, including patches whose circuit titles make node bodies wider than the repulsion radius
- **THEN** no two node bodies overlap: for every pair, the horizontal or the vertical centre distance is at least half the sum of the corresponding body extents

#### Scenario: Drag re-settle leaves no overlapping bodies

- **WHEN** the user drags a node of the force arrangement onto a neighbour and releases
- **THEN** the local re-settle plus its separation post-condition leave no overlapping body pairs among the bodies the pass may move (both endpoints within the re-settle radius and unpinned)
- **AND** nodes outside the re-settle radius and pinned nodes do not move

#### Scenario: A caged body is the one documented residue

- **WHEN** a dragged body ends up caged between bodies the pass may not move (pinned nodes, or nodes outside the re-settle radius) whose clearance is smaller than the dragged body's extent
- **THEN** the body keeps that intersection, and it is bounded to those user-fixed or out-of-neighbourhood bodies: an unmoved, unpinned pair is still separated, and the solver's own layouts remain clear

#### Scenario: Pinned bodies never overlap in the column arrangement

- **WHEN** the column arrangement is solved with pins carried from a force arrangement or a drag that intersect another body's position
- **THEN** the placed positions contain no overlapping body pairs
- **AND** a pin moves only when two pinned bodies themselves intersect

#### Scenario: Repulsion and cohesion knobs shape the force layout

- **WHEN** the force arrangement is solved with a different repulsion gain or a different cohesion than the default
- **THEN** the resulting positions differ from the defaults
- **AND** repeating the solve with the same knob tuple reproduces the same positions exactly
