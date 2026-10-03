# Spec Delta

## MODIFIED Requirements

### Requirement: ComfyUI-style rendering

Nodes shall render as rounded frames with a title bar; left-side input ports and right-side output ports; cable edges approximated with box-drawing characters, color-coded by cable type; cluster containers from banner groups surround their circuits.

- Circuit nodes render a rounded frame titled with the circuit name plus the instance index when repeated, colored by the `graph_node_*` circuit tokens.
- Controller nodes render a rounded frame titled with the panel label and chain ordinal (for example `P2B8 #1`), colored by `graph_node_controller`.
- Input-jack and output-jack nodes render a rounded frame titled with the register token (for example `I1`, `O3`), colored by `graph_node_jack_input` and `graph_node_jack_output`.
- Register edges (a circuit reading or writing a hardware register) render with the `graph_edge_register` token under the existing precedence: topology-error red, then diff, then latency, then cable kind.
- Node frames SHALL be sized in world units from the layout's per-node width estimate and projected through the camera, so a node's drawn frame scales with the zoom and never exceeds the space the arrangement reserved for it; corner radius, borders, ports, cable strokes, arrows, and cluster chrome SHALL scale with the same zoom, subject to minimum clamps.
- Node titles SHALL be fitted to the frame — font shrunk to fit, then ellipsized — and SHALL be omitted, together with port markers and cluster titles, when the frame falls below the legibility threshold.

#### Scenario: Node frame rendering

- **WHEN** the graph contains circuit, controller, input-jack, and output-jack nodes
- **THEN** each node renders as a rounded frame with its kind-specific title and color token.

#### Scenario: Edge color coding

- **WHEN** a cable of type "control" connects two circuits
- **THEN** the edge is rendered in cyan; "audio" in green; "midi" in magenta.

#### Scenario: Register edge color

- **WHEN** a circuit reads or writes a hardware register
- **THEN** the edge renders with the `graph_edge_register` token, and a topology-error or diff classification overrides it.

#### Scenario: Node frames scale with zoom

- **WHEN** the graph is displayed and the user zooms out or in
- **THEN** every node frame, its corner radius, borders, and ports scale with the zoom, so nodes never overlap on the column arrangement and never drift apart as undersized frames.

#### Scenario: Title fits the frame

- **WHEN** a node's title is wider than its frame at the base font size
- **THEN** the title is shrunk and ellipsized to fit the frame, and is omitted entirely once the frame is too small to read.

### Requirement: Pan and zoom navigation

The graph surface SHALL provide pan and zoom so the user can inspect a large layout at a legible scale. Zoom SHALL be driven by the mouse wheel (`+`/`-` step a preset scale) and pan by arrow keys, wheel-scroll on an overflowing layout, **or clicking the minimap**, reusing the existing physical-view camera model (zoom preset + pan offset). Zoom SHALL scale the node geometry with the camera — node frames, labels, ports, cable strokes, arrows, and cluster chrome — not only the spacing between nodes, and SHALL apply a level of detail that omits labels, port markers, and cluster titles once frames fall below a legibility threshold. The initial camera SHALL fit the graph against the actual visible pane (the graph slot rect in the class layout, the window canvas in the graph window), preserving aspect ratio and preferring to fill the canvas width, and SHALL frame the world bounds expanded by one node's world extent so whole node frames stay inside the pane. The zoom presets SHALL extend at least 2× below the fitted zoom, so zooming out from the fit reaches at least half the fitted scale. A zoom step SHALL be anchored at the visible pane's center.

#### Scenario: Zoom to legible scale

- **WHEN** a large patch's graph is spread beyond the available width and the user presses `+`
- **THEN** the view zooms in so nodes render larger, and the graph image is re-transmitted at the new scale

#### Scenario: Pan an overflowing graph

- **WHEN** the graph overflows the main area and the user presses an arrow key or scrolls
- **THEN** the view pans in that direction and the graph image is re-transmitted at the new offset

#### Scenario: Legible initial fit

- **WHEN** a graph opens without user pan/zoom
- **THEN** the camera frames the graph against the visible pane's actual size (not a fixed viewport), preserving aspect ratio and preferring to fill the canvas width, so the smallest node still renders at a readable width (no node collapses to 1–2 characters)

#### Scenario: Zoom-out reaches below half the fitted zoom

- **WHEN** the graph is at the fitted zoom and the user presses `-` repeatedly
- **THEN** the zoom steps below half the fitted scale, and each step is anchored at the visible pane's center so the content under the center stays put

#### Scenario: Zoom anchor uses the visible pane center

- **WHEN** the graph pane is smaller than the app window and the user presses `+` or `-`
- **THEN** the zoom is anchored at the graph pane's center, not the app window's center or the world origin

#### Scenario: Minimap click pans camera to clicked location

- **WHEN** graph pane is open and minimap is visible, AND user left-clicks inside the minimap panel
- **THEN** graph camera pans so the clicked minimap position maps to the world center of the visible viewport, preserving current zoom level

#### Scenario: Zoom scales node bodies

- **WHEN** the graph is at the fitted zoom and the user presses `-` (or zooms out with the wheel)
- **THEN** node frames and the spacing between them shrink by the same ratio, so the graph reads as a compact structure with no overlapping frames, and labels are omitted once frames fall below the legibility threshold

#### Scenario: Fit keeps whole node frames visible

- **WHEN** the graph opens or the user presses the fit-and-center key
- **THEN** the fit frames the world bounds expanded by one node's world extent, so every full node frame lies inside the visible pane
