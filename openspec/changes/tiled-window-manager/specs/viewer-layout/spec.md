## MODIFIED Requirements

### Requirement: Embedded source pane layout

The source viewer SHALL render as a slot in the right column of the tiled layout, not as a fixed split pane. When the source viewer is the only view in the right column, it shares the width split ratio with the left panel pane (default 60% panels / 40% source). When multiple views share the right column, the source viewer occupies one horizontal slot and its height is determined by the number of visible slots. The source pane's internal two-pane layout (sidebar + main area) remains unchanged.

#### Scenario: Default split with source only
- **WHEN** the user opens the source viewer as the only view in the right column
- **THEN** the panels column takes 60% of the width and the source pane takes 40%

#### Scenario: Source with other views
- **WHEN** the source viewer shares the right column with the graph view
- **THEN** the source pane occupies one horizontal slot in the right column at the configured split ratio

### Requirement: Pane focus indication

Exactly one pane SHALL be focused at a time; the focused pane SHALL be visually emphasized via the `pane_focus_border` theme token. When the source pane is focused, its border renders with the focus token; when focus moves away, the border returns to `pane_unfocused_border`.

#### Scenario: Focus follows Tab
- **WHEN** the source pane is focused and the user presses `Tab`
- **THEN** focus moves to the next pane in the carousel and the source pane's border updates to unfocused styling

## REMOVED Requirements

### Requirement: Two-pane circuit layout (proportions)
**Reason**: The outer split ratio is now controlled by the tiling window manager; the source pane's internal sidebar/main-area proportions remain but the outer width is no longer self-managed.
**Migration**: The source pane's width is determined by the tiling layout's split ratio. The internal sidebar (circuit list) retains its `max(20, width / 5)` calculation based on the source pane's allocated width.

### Requirement: Viewer status bar
**Reason**: The tiled layout's status bar consolidates hints; the viewer-specific status bar is no longer needed as a separate element.
**Migration**: Viewer hints appear in the shared status bar when the source pane has focus.
