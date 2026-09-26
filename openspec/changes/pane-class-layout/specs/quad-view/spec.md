# quad-view Specification (Delta)

## REMOVED Requirements

### Requirement: Quad concurrent layout
The system SHALL NOT render four panes concurrently (panels, source, graph FULL, graph FILTERED) as a fixed 2x2 grid.

**Reason**: Superseded by the class-based three-pane layout, which fixes three panes and places views by class.
**Migration**: Open the graph in the big pane and the source viewer in a small pane. Influence and dependency filtering stay on the graph surface through the `i` and `f` keys.

### Requirement: Highlight vs dim in FULL graph
The requirement that a FULL graph pane renders influenced nodes and edges highlighted and the rest dimmed SHALL NOT be carried by this capability.

**Reason**: The behavior belongs to the graph surface rather than to the quad arrangement, and it survives the quad view.
**Migration**: No user action required. The graph surface keeps the influence highlight and dim styling through its influence filter.

### Requirement: Filtered graph re-solves compactly for readability
The requirement that a FILTERED graph pane solves the induced subgraph on its own with a compact fit SHALL NOT be carried by this capability.

**Reason**: The behavior belongs to the graph surface's influence and dependency filters, which survive the quad view.
**Migration**: No user action required. The graph surface still solves a filtered subset as its own layout with a fresh camera fit.

### Requirement: Focus cycle and keys in quad mode
The system SHALL NOT cycle focus across four quad panes with `Tab` and SHALL NOT close quad with `Esc`.

**Reason**: Replaced by pane focus cycling over the three-pane tree.
**Migration**: `Tab`/`Shift+Tab` cycle the big pane and the two small panes. `Esc` closes the focused pane's view.

### Requirement: Kitty-gfx optional polish
The system SHALL NOT attempt kitty inline-image rendering for quad panes.

**Reason**: The quad arrangement is gone, so the condition it attached to no longer exists.
**Migration**: No user action required. Graph rendering is unchanged for the panes that remain.

### Requirement: Visual validation for quad
The system SHALL NOT require gallery and snapshot coverage of quad-view frames per theme and width.

**Reason**: The quad arrangement is gone, so there are no quad frames to cover.
**Migration**: Visual validation covers the three-pane layout instead.
