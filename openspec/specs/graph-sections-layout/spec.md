# graph-sections-layout Specification

## Purpose
Provides a sections-mode joint black-hole solver for the signal-flow graph: sections act as invisible fixed anchors ("black holes") computed from patch order, attracting their member circuits with soft springs while cable springs pull across sections and size-aware repulsion prevents overlap. Live sliders control slice/section/cable forces, and an Arrange button re-runs the joint solve from current positions.

## Requirements

### Requirement: Sections layout mode

The signal-flow graph SHALL support a third layout mode `sections` alongside `column` and `force`, selectable via `[layout] mode = "sections"` and the existing `h` toggle / `a` cycle in the graph pane.

#### Scenario: Selecting sections mode via config

- **WHEN** `[layout] mode = "sections"` is set in config.toml and a graph is opened
- **THEN** the graph opens in sections mode, not column or force

#### Scenario: Cycling into sections mode

- **WHEN** the graph pane is focused and the user presses `h` (or `a` cycles arrangements)
- **THEN** sections mode appears in the cycle after force mode, and selecting it switches to the joint black-hole solver

### Requirement: Sections-mode joint black-hole solver

In sections mode, the graph SHALL use a joint solve where:
- Each circuit starts in a patch-order slice (at least one slice per circuit, arranged left→right)
- Each banner group forms an invisible fixed anchor ("black hole") at the mean slice of its members, computed once from patch order
- Members feel a soft spring toward their section's black hole
- Cable springs pull across sections (bundle-normalized, tension-adjustable)
- Size-aware grid-hashed repulsion prevents overlap using `node_world_sizes`
- Cluster boxes are not drawn (sections are visible only through member clustering)

#### Scenario: Members cluster around their section hole

- **WHEN** a banner group has multiple member circuits in sections mode
- **THEN** those circuits render clustered around their section's computed anchor position

#### Scenario: Cable springs pull across sections

- **WHEN** a cable connects circuits in different sections
- **THEN** the cable spring pulls the connected circuits across section boundaries, visible as stretched edges

#### Scenario: No cluster boxes drawn in sections mode

- **WHEN** sections mode is active
- **THEN** no banner-group enclosing rectangles are rendered (the clustering itself visualizes sections)

### Requirement: Live force sliders

In sections mode, the graph pane SHALL display a toggleable sliders panel (top corner) with three draggable sliders:
- Slice force (stiffness of circuit→slice-center springs)
- Section force (stiffness of member→black-hole springs)
- Cable force (the existing tension, exposed here for unified tuning)

Each slider SHALL have a visible range and default value. Dragging a slider SHALL re-run the joint solve immediately with the new values and repaint.

#### Scenario: Dragging slice force spreads members across slice borders

- **WHEN** sections mode is active and the user lowers the slice force slider
- **THEN** circuits drift toward their section holes and cable springs pull them across slice boundaries more visibly

#### Scenario: Dragging section force tightens clusters

- **WHEN** sections mode is active and the user raises the section force slider
- **THEN** member circuits cluster more tightly around their section holes

#### Scenario: Cable force is the existing tension

- **WHEN** sections mode is active and the user adjusts the cable force slider
- **THEN** the behavior is identical to `Alt+[`/`Alt+]` on the force/column modes, synced

### Requirement: Arrange button

In sections mode, the sliders panel SHALL include an Arrange button. Pressing it SHALL re-run the joint solve from the current positions (anneal) and freeze the result, reporting "Arranged (sections)" in the status line.

#### Scenario: Arrange re-solves from current positions

- **WHEN** the user presses the Arrange button in sections mode
- **THEN** the joint solve runs from the current node positions (not the seed) and freezes at the energy threshold

### Requirement: Centering works in sections mode

The `c` key (center graph in pane) and `Shift+c` (refit and center) SHALL work correctly in sections mode, using the published live canvas rect.

#### Scenario: Wide graph centers in sections mode

- **WHEN** a sections-mode graph is wider than the pane and the user presses `c`
- **THEN** the camera pans to center the graph horizontally without changing zoom
