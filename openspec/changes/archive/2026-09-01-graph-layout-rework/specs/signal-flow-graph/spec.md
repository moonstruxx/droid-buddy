## ADDED Requirements

### Requirement: Manual pin and drag-to-place

The system SHALL let the user pin circuit nodes so manual placements survive the solver. Pinned nodes SHALL be fixed anchors the solver never moves. The `p` key SHALL toggle pin/unpin on the hovered graph node, and dragging a node SHALL place it as a fixed anchor (auto-pin at the dropped position). The first circuit in `.ini` file order SHALL be pinned by default as the graph's tip. Unpinning a node SHALL release it to re-flow.

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

## MODIFIED Requirements

### Requirement: Convergence-based layout

The force-directed layout solver SHALL bias the layout toward a single dominant left→right axis: initial positions seed by topological layer (depth → x, within-layer order → y) and the solver converges to a horizontal chain. Cable spring force SHALL dominate the repulsion force so connected circuits cohere into a readable flow. The solver SHALL run bounded iterations until total kinetic energy falls below a threshold, then freeze positions. The solver SHALL be re-invoked only on patch load or when the user drags a node; no continuous tick or drift occurs. Pinned nodes (including the tip, the first circuit in `.ini` order) SHALL act as fixed anchors the solver never moves.

#### Scenario: Layout converges and freezes

- **WHEN** a patch with 60 circuits is loaded and the graph view opens
- **THEN** the layout runs for a bounded number of iterations until energy < threshold, then freezes; positions do not change on subsequent redraws unless the patch reloads or a node is dragged.

#### Scenario: Re-solve on node drag

- **WHEN** the user drags a node in the graph view
- **THEN** the solver re-invokes (damped, local re-settle) from the new position; other nodes settle quickly without a full global re-run, and pinned nodes stay put.

#### Scenario: Layout converges to a horizontal chain

- **WHEN** a patch with 60 circuits is loaded and the graph view opens on a wide terminal
- **THEN** the layout converges along a single left→right axis and fills the canvas width, rather than stacking vertically.

#### Scenario: Cable springs pull circuits together

- **WHEN** two circuits are connected by a cable
- **THEN** they sit nearer each other than unconnected circuits, so edges read as springs.

#### Scenario: Tip stays anchored

- **WHEN** the graph converges
- **THEN** the first circuit (`.ini` order) remains at the left and does not drift during the solve or on subsequent redraws.

### Requirement: Banner-range grouping

Comment banners (`# ---- Name ----`) own a range of circuits from themselves to the next banner or end of file. The graph renders each such range as a cluster container with the banner name as the cluster label. Cluster members SHALL cohere through an internal cohesion force and render inside an enclosing rectangle, so clusters are content containers rather than stiff layout stripes.

#### Scenario: Banner groups circuits

- **WHEN** a patch has `# ---- Pulsar clock ----` followed by `[clock32in]`, `[threeCV1]`, and later `# ---- Steady clock ----` with `[osc1]`
- **THEN** the graph renders two cluster containers labeled "Pulsar clock" and "Steady clock", each owning its circuits.

#### Scenario: Cluster members cohere in enclosing rectangles

- **WHEN** the graph renders a banner group's circuits
- **THEN** they are drawn inside an enclosing rectangle and cohere together rather than being stretched into a vertical stripe.

### Requirement: Kitty-graphics graph rendering

The `kitty-gfx` feature SHALL be part of the default feature set. When the terminal supports the kitty graphics protocol and `kitty-gfx` is enabled, the graph surface SHALL render the signal-flow graph via the kitty graphics protocol by default: circuit nodes render as anti-aliased rounded rectangles with a title bar, cable edges render as anti-aliased colored curves with direction arrows, and labels render as rasterized text. The image SHALL be composited beneath the header, status, and picker text so those text cells remain readable above it.

When the terminal does not support kitty graphics (or the feature is off), the graph SHALL fall back to the existing box-drawing renderer without error.

#### Scenario: Image renderer in a kitty terminal

- **WHEN** the terminal supports kitty graphics, the default feature set is built, and the graph surface opens
- **THEN** the graph renders by default as an anti-aliased image with rounded-rect nodes and curved labeled edges, and each circuit name is legible as rasterized text

#### Scenario: Fallback without kitty support

- **WHEN** the terminal does not support the kitty graphics protocol (or the feature is disabled)
- **THEN** the graph renders via the existing box-drawing renderer, and no error is surfaced

#### Scenario: Image sits below text layer

- **WHEN** the graph image is composited and the header or status bar is visible
- **THEN** the image cells are positioned beneath the header/status text, so the text renders on top of (not behind) the graph

### Requirement: Pan and zoom navigation

The graph surface SHALL provide pan and zoom so the user can inspect a large layout at a legible scale. Zoom SHALL be driven by the mouse wheel (`+`/`-` step a preset scale) and pan by arrow keys or wheel-scroll on an overflowing layout, reusing the existing physical-view camera model (zoom preset + pan offset). The initial camera SHALL fit the graph preserving aspect ratio and preferring to fill the canvas width, on both the box and kitty render paths, such that the smallest node renders at a readable width.

#### Scenario: Zoom to legible scale

- **WHEN** a large patch's graph is spread beyond the available width and the user presses `+`
- **THEN** the view zooms in so nodes render larger, and the graph image is re-transmitted at the new scale

#### Scenario: Pan an overflowing graph

- **WHEN** the graph overflows the main area and the user presses an arrow key or scrolls
- **THEN** the view pans in that direction and the graph image is re-transmitted at the new offset

#### Scenario: Legible initial fit

- **WHEN** a graph opens without user pan/zoom
- **THEN** the camera frames the graph preserving aspect ratio and preferring to fill the canvas width, so the smallest node still renders at a readable width (no node collapses to 1–2 characters)
