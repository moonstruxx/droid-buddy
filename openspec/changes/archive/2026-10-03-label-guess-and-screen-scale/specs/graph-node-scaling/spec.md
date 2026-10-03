# Spec Delta

## Purpose

Node bodies, labels, ports, cable strokes and cluster chrome scale with the graph camera zoom, so a zoomed-out graph reads as a compact non-overlapping structure and a zoomed-in graph magnifies detail — with labels now living at their own screen-space size level.

## MODIFIED Requirements

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
