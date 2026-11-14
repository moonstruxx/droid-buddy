# Spec Delta

## MODIFIED Requirements

### Requirement: Level of detail at extreme zoom

Node labels SHALL render at a fixed screen-space font size independent of the camera zoom: the label font SHALL NOT shrink when zooming out nor grow when zooming in. Label text wider than the node's frame SHALL be ellipsized to the frame at that fixed size, and a node's label SHALL be omitted when its frame is narrower than one character of the label font, so a zoomed-out graph does not render unreadable slivers. Port markers SHALL be omitted below the legibility threshold.

Cluster titles SHALL render at a fixed screen-space font size independent of the camera zoom, like node labels, and SHALL be omitted only when their enclosing cluster box has no room for the text in screen space — a box whose drawn height cannot hold one title line, or whose drawn width cannot hold a readable fragment of the title, renders without a title. Node frames SHALL scale linearly with the camera zoom and SHALL NOT be clamped to a minimum size, so frames never overlap at any zoom; cable strokes, borders, arrows, and cluster chrome SHALL keep a minimum render size so the graph structure stays visible at the zoom floor.

#### Scenario: Label size independent of zoom

- **WHEN** the user zooms between the highest and lowest presets
- **THEN** every rendered node label uses the same font size in points while node frames scale with the zoom

#### Scenario: Cluster title size independent of zoom

- **WHEN** the user zooms between the fit zoom and the highest preset on a graph whose cluster boxes have room for their titles
- **THEN** every rendered cluster title uses the same font size in points while the cluster boxes scale with the zoom

#### Scenario: Cluster titles stay readable at fit zoom

- **WHEN** a large graph opens fitted at a zoom low enough that its cluster boxes are drawn far smaller than their world size, and a box still has room for its title
- **THEN** that box paints its title at the fixed screen-space size instead of omitting it

#### Scenario: Cluster titles yield to boxes with no room

- **WHEN** a cluster box is drawn too small in screen space to hold one title line, or too narrow to hold a readable fragment of the title
- **THEN** that box renders without a title, and no partial or unreadable title text is painted

#### Scenario: Long label fits its frame

- **WHEN** a node's title is wider than its frame at the fixed label font size
- **THEN** the text is ellipsized rather than overflowing the frame, and the font size is unchanged

#### Scenario: Labels hidden when frames are too small

- **WHEN** the user zooms out until a node's frame is narrower than one character of the label font
- **THEN** that node renders with no title text, while cluster titles follow their own box-room rule

#### Scenario: Structure stays visible at the zoom floor

- **WHEN** the user zooms to the minimum preset
- **THEN** node frames scale down to sub-pixel marks without overlapping, and cable strokes, borders, and cluster chrome stay at their minimum render size so the graph structure remains visible
