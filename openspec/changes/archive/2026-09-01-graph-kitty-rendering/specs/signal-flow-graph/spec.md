# Signal-Flow Graph Specification

## ADDED Requirements

### Requirement: Kitty-graphics graph rendering

When the terminal supports the kitty graphics protocol and the `kitty-gfx` feature is enabled, the graph surface SHALL render the signal-flow graph via the kitty graphics protocol instead of box-drawing characters: circuit nodes render as anti-aliased rounded rectangles with a title bar, cable edges render as anti-aliased colored curves with direction arrows, and labels render as rasterized text. The image SHALL be composited beneath the header, status, and picker text so those text cells remain readable above it.

When the terminal does not support kitty graphics (or the feature is off), the graph SHALL fall back to the existing box-drawing renderer without error.

#### Scenario: Image renderer in a kitty terminal

- **WHEN** the terminal supports kitty graphics, `kitty-gfx` is enabled, and the graph surface opens
- **THEN** the graph renders as an anti-aliased image with rounded-rect nodes and curved labeled edges, and each circuit name is legible as rasterized text

#### Scenario: Fallback without kitty support

- **WHEN** the terminal does not support the kitty graphics protocol (or the feature is disabled)
- **THEN** the graph renders via the existing box-drawing renderer, and no error is surfaced

#### Scenario: Image sits below text layer

- **WHEN** the graph image is composited and the header or status bar is visible
- **THEN** the image cells are positioned beneath the header/status text, so the text renders on top of (not behind) the graph

### Requirement: Pan and zoom navigation

The graph surface SHALL provide pan and zoom so the user can inspect a large layout at a legible scale. Zoom SHALL be driven by the mouse wheel (`+`/`-` step a preset scale) and pan by arrow keys or wheel-scroll on an overflowing layout, reusing the existing physical-view camera model (zoom preset + pan offset). The initial camera SHALL fit the graph such that nodes render at a readable minimum size rather than collapsing to sub-character width.

#### Scenario: Zoom to legible scale

- **WHEN** a large patch's graph is spread beyond the available width and the user presses `+`
- **THEN** the view zooms in so nodes render larger, and the graph image is re-transmitted at the new scale

#### Scenario: Pan an overflowing graph

- **WHEN** the graph overflows the main area and the user presses an arrow key or scrolls
- **THEN** the view pans in that direction and the graph image is re-transmitted at the new offset

#### Scenario: Legible initial fit

- **WHEN** a graph opens without user pan/zoom
- **THEN** the camera frames the graph so the smallest node still renders at a readable width (no node collapses to 1–2 characters)

### Requirement: Interactions preserved on the image renderer

The system SHALL preserve every existing graph interaction when the image renderer is active: left-button node drag (re-settle + `NodeMoved`), hover highlight, `x` per-circuit processing disable, `e` label overlay, diff coloring, latency ramp coloring, and topology-error edge highlight. The published `graph_node_rects` SHALL be derived from the same camera the image uses so pointer hit-testing stays aligned with what was drawn.

#### Scenario: Drag a node on the image path

- **WHEN** the image renderer is active and the user drags a node
- **THEN** the node moves with the cursor, the local neighborhood re-settles, and `NodeMoved` is emitted — identical to box-drawing behavior

#### Scenario: Diff and latency coloring still apply

- **WHEN** the image renderer is active and a diff report or latency ramp is present
- **THEN** added/removed/changed edges and the latency ramp render with the same tokens as on the box-drawing path, and topology-error edges stay red

#### Scenario: Disable still works on the image path

- **WHEN** the image renderer is active, a node is hovered, and the user presses `x`
- **THEN** that circuit instance is disabled, the graph rebuilds, and the node/edges render dimmed

### Requirement: Theme colors map to RGB

The pixel renderer SHALL derive every node, edge, and label color from the active theme's semantic color tokens (`Color` → RGB), never from hardcoded RGB values. The existing color precedence (topology-error red > diff classification > latency ramp > cable kind) SHALL be preserved exactly in the pixel path.

#### Scenario: No hardcoded RGB

- **WHEN** the image renderer colors a node or edge
- **THEN** the RGB used derives from the active theme token (error/diff/ramp/kind), matching the box-drawing path's classification

#### Scenario: Error stays red

- **WHEN** an edge carries a topology-error finding
- **THEN** it renders in the error token (red) on the image path, above any diff or latency coloring
