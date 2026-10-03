# graph-node-scaling Specification

## Purpose

Node bodies, labels, ports, cable strokes and cluster chrome scale with the graph camera zoom, so a zoomed-out graph reads as a compact non-overlapping structure and a zoomed-in graph magnifies detail.

## Requirements

### Requirement: Zoom-proportional node geometry

Node bodies SHALL be sized in world units and projected through the camera, so a node's pixel size is its world size multiplied by the camera zoom. A node's world width SHALL come from the layout's per-node width estimate — the same estimate the column arrangement reserves for that node — and its world height SHALL be a fixed constant that fits the layout's vertical slot. Corner radius, border width, and port markers SHALL scale with the same zoom, subject to minimum clamps.

#### Scenario: Zoom out shrinks node frames

- **WHEN** the graph is displayed and the user zooms out
- **THEN** every node frame's pixel width and height shrink in proportion to the zoom ratio, and the spacing between nodes scales by the same ratio.

#### Scenario: Zoom in magnifies node frames

- **WHEN** the user zooms in
- **THEN** node frames, their corner radius, borders, and port markers render larger in proportion, and cable strokes scale with them.

#### Scenario: Column arrangement never overlaps at any zoom

- **WHEN** a graph with many circuits is displayed on the column arrangement at any zoom preset
- **THEN** no two node frames overlap, because each node's world width does not exceed the width the column arrangement reserved for it and frames are never clamped above their proportional size.

### Requirement: Level of detail at extreme zoom

Node labels SHALL render at a fixed screen-space font size independent of the camera zoom: the label font SHALL NOT shrink when zooming out nor grow when zooming in. Label text wider than the node's frame SHALL be ellipsized to the frame at that fixed size, and a node's label SHALL be omitted when its frame is narrower than one character of the label font, so a zoomed-out graph does not render unreadable slivers. Port markers and cluster titles SHALL be omitted below the legibility threshold. Node frames SHALL scale linearly with the camera zoom and SHALL NOT be clamped to a minimum size, so frames never overlap at any zoom; cable strokes, borders, arrows, and cluster chrome SHALL keep a minimum render size so the graph structure stays visible at the zoom floor.

#### Scenario: Label size independent of zoom

- **WHEN** the user zooms between the highest and lowest presets
- **THEN** every rendered node label uses the same font size in points while node frames scale with the zoom

#### Scenario: Long label fits its frame

- **WHEN** a node's title is wider than its frame at the fixed label font size
- **THEN** the text is ellipsized rather than overflowing the frame, and the font size is unchanged

#### Scenario: Labels hidden when frames are too small

- **WHEN** the user zooms out until a node's frame is narrower than one character of the label font
- **THEN** that node renders with no title text; port markers and cluster titles follow their own legibility threshold

#### Scenario: Structure stays visible at the zoom floor

- **WHEN** the user zooms to the minimum preset
- **THEN** node frames scale down to sub-pixel marks without overlapping, and cable strokes, borders, and cluster chrome stay at their minimum render size so the graph structure remains visible

### Requirement: Fit frames whole node bodies

The fit camera SHALL frame the world bounds expanded by one node's world extent on each axis, so a fit that frames the whole graph keeps every node's full frame inside the visible pane, not only the node's center point.

#### Scenario: Fit keeps whole frames on canvas

- **WHEN** a graph is opened, or the user presses the fit-and-center key
- **THEN** every node's full frame lies inside the visible pane.

#### Scenario: Fit still frames a graph that overflows

- **WHEN** the graph is larger than the visible pane in at least one axis
- **THEN** the fit zoom frames the whole graph extent and the graph overflows the pane rather than being clipped at its edges.

### Requirement: Hit-testing follows zoomed geometry

Pointer hit-testing SHALL test against the same world-space node extents the renderer draws, so the hit area is independent of the camera zoom, and SHALL apply a minimum pixel hit size so a node stays selectable when its drawn frame is only a few pixels.

#### Scenario: Hit-test at low zoom

- **WHEN** the camera is zoomed out so a node's drawn frame is a few pixels wide and the user clicks its center
- **THEN** that node becomes hovered and selected.

#### Scenario: Hit-test at high zoom

- **WHEN** the camera is zoomed in and the user clicks inside a node's drawn frame
- **THEN** that node is selected, while a click in the gap between two node frames selects nothing.
