# Signal-Flow Graph Specification

## Purpose

A signal-flow node-graph view of DROID patches: circuits as nodes, virtual cables as edges, ComfyUI-style rendering, force-directed convergence layout, and interaction (view-switch, node drag, topology validation).

## Requirements

### Requirement: Virtual-cable extraction
The system SHALL extract virtual cables from parsed circuit sections:

- `output = _NAME` on a circuit creates a cable source.
- Any circuit param `= _NAME` (embedded in arithmetic expressions) references a cable sink.
- Cables in comment lines (`# output = _X`) are ignored.
- A cable source may fan out to any number of sinks (1 → n topology is valid); `n → 1` is invalid and shall be flagged.

#### Scenario: Cable creation from output param
- **WHEN** a circuit section contains `output = _PULSARCLOCK`
- **THEN** a cable source `_PULSARCLOCK` is registered with this circuit as the origin.

#### Scenario: Cable consumption from input param
- **WHEN** a circuit section contains `input = _PULSARCLOCK` (or `frequency = _PULSARCLOCK * 2 - _BASE`)
- **THEN** the cable sink `_PULSARCLOCK` is registered, linking to the source circuit.

#### Scenario: Invalid topology n → 1 flagged
- **WHEN** multiple circuits drive `input = _SINGLE_CLOCK` and no circuit outputs `_SINGLE_CLOCK`
- **THEN** the graph highlights an invalid `n → 1` topology error state.

### Requirement: Banner-range grouping
Comment banners (`# ---- Name ----`) own a range of circuits from themselves to the next banner or end of file. The graph renders each such range as a cluster container with the banner name as the cluster label.

#### Scenario: Banner groups circuits
- **WHEN** a patch has `# ---- Pulsar clock ----` followed by `[clock32in]`, `[threeCV1]`, and later `# ---- Steady clock ----` with `[osc1]`
- **THEN** the graph renders two cluster containers labeled "Pulsar clock" and "Steady clock", each owning its circuits.

### Requirement: Convergence-based layout
The force-directed layout solver shall run bounded iterations until total kinetic energy falls below a threshold, then freeze positions. The solver shall be re-invoked only on patch load or when the user drags a node; no continuous tick or drift occurs.

#### Scenario: Layout converges and freezes
- **WHEN** a patch with 60 circuits is loaded and the graph view opens
- **THEN** the layout runs for a bounded number of iterations until energy < threshold, then freezes; positions do not change on subsequent redraws unless the patch reloads or a node is dragged.

#### Scenario: Re-solve on node drag
- **WHEN** the user drags a node in the graph view
- **THEN** the solver re-invokes (damped, local re-settle) from the new position; other nodes settle quickly without a full global re-run.

### Requirement: ComfyUI-style rendering
Nodes shall render as rounded frames with a title bar; left-side input ports and right-side output ports; cable edges approximated with box-drawing characters, color-coded by cable type; cluster containers from banner groups surround their circuits.

#### Scenario: Node frame rendering
- **WHEN** the graph renders a circuit node
- **THEN** a rounded frame appears with the circuit name in the title bar, left ports on the left edge, right ports on the right edge.

#### Scenario: Edge color coding
- **WHEN** a cable of type "control" connects two circuits
- **THEN** the edge is rendered in cyan; "audio" in green; "midi" in magenta.

### Requirement: Edge color from cable kind
The system SHALL color a graph edge by the declared `cable_kind` of the producing circuit when present, falling back to name-substring inference when absent. This preserves the existing classification of all embedded circuits byte-for-byte while letting plugin circuits declare their kind.

#### Scenario: Plugin circuit with declared kind
- **WHEN** a graph edge is produced by a plugin circuit that declares `cable_kind`
- **THEN** the edge renders with the token for that kind (control/audio/midi), regardless of the circuit's name substrings

#### Scenario: Embedded circuit unchanged
- **WHEN** a graph edge is produced by an embedded circuit with no declared kind
- **THEN** the edge renders exactly as before this change (substring inference)

### Requirement: Node color for plugin circuits
The system SHALL color a graph node for a plugin circuit using the circuit's declared `color` when present, falling back to substring inference otherwise.

#### Scenario: Plugin circuit with declared color
- **WHEN** a graph node is a plugin circuit that declares a `color` token
- **THEN** the node renders with that token

#### Scenario: Plugin circuit without declared color
- **WHEN** a graph node is a plugin circuit with no declared `color`
- **THEN** the node renders via the existing name-inference path, with no error

### Requirement: View-switch key
The system SHALL provide a `g g` key sequence to open the signal-flow graph view (while in normal mode, no patch loaded or a patch loaded). `Esc` closes the graph and returns focus to the controller panels.

#### Scenario: Open graph with g g
- **WHEN** in normal mode with a patch loaded, the user presses `g` then `g`
- **THEN** the graph view opens alongside (or in place of, depending on layout) the controller panels, showing the signal-flow nodes and edges.

#### Scenario: Close graph with Esc
- **WHEN** the graph view is open and the user presses `Esc`
- **THEN** the graph view closes, focus returns to the controller panels, and any component selection is preserved.

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

### Requirement: Graph reflects disabled circuits

In the graph surface, a circuit instance with processing disabled SHALL render its node frame and its incident edges dimmed, overriding any influence highlight on those edges. Hovering a node SHALL keep normal hover styling. The `x` key SHALL toggle processing for the hovered node's circuit instance. Toggling SHALL rebuild the graph and recompute influence, emitting `GraphRebuilt`.

#### Scenario: Disabled node renders dim

- **WHEN** a circuit instance has processing disabled and the graph is rebuilt
- **THEN** that node's frame and all edges incident to it render dim, while enabled nodes and their edges render normally.

#### Scenario: Toggle via x key

- **WHEN** the graph surface is open and a node is hovered
- **THEN** pressing `x` disables (or re-enables) that circuit instance, rebuilds the graph, and shows a status message naming the circuit.

#### Scenario: Influence highlight yields to disabled

- **WHEN** a disabled circuit's edge would otherwise carry an influence highlight
- **THEN** the edge renders dim instead.

### Requirement: Graph node titles use circuit label override
The system SHALL render graph node titles (FULL and FILTERED panes) with the circuit-instance label when present, otherwise the circuit name + instance index, without changing graph physics or layout.

#### Scenario: Graph node override
- **WHEN** `circuits."motorfader:12"` is `T1 Accu` and the node is hovered
- **THEN** both FULL and FILTERED panes show `T1 Accu` as the node title and `hovered_graph_node` targeting is unchanged

### Requirement: Latency optimizer menu (`g o`)

The graph surface MUST provide a `g o` key chord that generates latency-optimized candidate orderings (via the `latency-optimizer` capability) and opens a candidate menu overlay. With no patch loaded it MUST show a status hint instead.

- The menu lists up to 3 candidates, best first: variant label + `avg X→Y · max A→B · back-edges N→M` (before → after).
- `j`/`k` navigate; `Enter` previews the selected candidate in memory (graph recolors via the latency ramp); `s` exports it save-as; `r` restores the original order; `Esc` closes the menu (restoring the original order if a preview is active).
- While the menu is open it owns all keys (mirroring the validation-modal priority).
- The status line MUST show the active candidate label while a preview is loaded.

#### Scenario: Open menu with candidates

Given a loaded patch with cables and `g o` pressed, the menu shows the generated candidates with before/after summaries.

#### Scenario: No patch loaded

Given no patch and `g o` pressed, the status line shows a hint that no patch is loaded; no menu opens.

#### Scenario: Preview then Esc restores

Given a candidate previewed and `Esc` pressed, the menu closes and the patch returns to its original section order and coloring.

#### Scenario: Export from menu

Given a candidate selected and `s` pressed, the reordered patch is written save-as (see `patch-writing`), the source is untouched, and the status confirms the written path.

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
