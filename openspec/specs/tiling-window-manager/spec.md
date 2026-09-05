# Tiling Window Manager Specification

## Purpose

A tiling window manager for the main band: panels always occupy the left pane; open views (graph, source viewer, physical layout) stack in a right column with horizontal cuts; a carousel rotates views through slots; focus routing ensures keys reach the active pane.

## Requirements

### Requirement: Tiled main band layout

The main area (between header and status) SHALL render as a two-column layout: a full-height left pane showing the hardware component panels, and a right column where open views are placed with horizontal cuts. Only one view occupies a slot at a time. The default layout when a patch is loaded shows only the left panel pane (right column empty). Opening a view (graph via `g g`, source viewer via `g v`, physical skeleton via `s`) places it as the first slot in the right column. Opening a second view adds it below the first, splitting the right column horizontally. A third view adds a third horizontal slot. The maximum number of visible slots is three; a fourth view replaces the oldest non-focused slot.

#### Scenario: Single view in right column

- **WHEN** a patch is loaded and the user opens the graph view via `g g`
- **THEN** the main area shows the panel pane on the left and the graph in the right column, each taking 50% width

#### Scenario: Two views in right column

- **WHEN** the graph is open in the right column and the user opens the source viewer via `g v`
- **THEN** the right column splits horizontally: graph takes the top half, source viewer the bottom half, each sharing 50% of the right column's height

#### Scenario: Third view triggers split

- **WHEN** two views are open and the user opens a third view
- **THEN** the right column divides into three horizontal slots of equal height

#### Scenario: Fourth view replaces oldest non-focused

- **WHEN** three views are open and the user opens a fourth view
- **THEN** the oldest non-focused slot is replaced by the new view

### Requirement: Carousel rotation

The system SHALL provide a carousel to rotate views through each slot in the right column. `Tab` cycles forward through the available view types (panels → graph → source viewer → physical → panels...); `Shift+Tab` cycles backward. When a slot contains a view, cycling replaces it with the next view type in the carousel. If a view type is already open in a different slot, cycling to it moves focus to that slot without duplicating the view. The carousel order is: panels (always left, not cycled), graph, source viewer, physical layout.

#### Scenario: Tab cycles to next view

- **WHEN** the right column contains the graph view and the user presses `Tab`
- **THEN** the graph slot is replaced with the source viewer, and focus moves to it

#### Scenario: Shift+Tab cycles backward

- **WHEN** the right column contains the source viewer and the user presses `Shift+Tab`
- **THEN** the source viewer slot is replaced with the graph view

#### Scenario: Focus moves to existing view

- **WHEN** the graph is in slot 1 and the source viewer is in slot 2, and the user cycles to graph
- **THEN** focus moves to slot 1 (graph) without creating a duplicate

### Requirement: Focus routing

Exactly one pane SHALL be focused at a time. `Tab` cycles focus across all visible panes (left panel pane, then each slot in the right column top-to-bottom). The focused pane's border SHALL render with the `pane_focus_border` theme token; unfocused panes use `pane_unfocused_border`. Keyboard input routes to the focused pane's handler: graph keys when graph is focused, viewer keys when viewer is focused, panel keys when panels are focused. Mouse clicks on a pane set focus to that pane. When no pane is explicitly focused (initial state), the left panel pane receives input.

#### Scenario: Tab cycles focus

- **WHEN** three panes are visible (panels, graph, source viewer) and the user presses `Tab` repeatedly
- **THEN** focus cycles panels → graph → source viewer → panels...

#### Scenario: Focus border indicates active pane

- **WHEN** the graph pane is focused
- **THEN** the graph pane's border renders with the `pane_focus_border` token and other panes use `pane_unfocused_border`

#### Scenario: Mouse click sets focus

- **WHEN** the source viewer is focused and the user clicks inside the graph pane
- **THEN** focus moves to the graph pane

#### Scenario: Keys route to focused pane

- **WHEN** the graph pane is focused and the user presses `x` (toggle processing)
- **THEN** the hovered graph node's processing toggles; if panels were focused, `x` would have no effect

### Requirement: Split ratio adjustment

The `[` and `]` keys SHALL adjust the horizontal split ratio between the left panel pane and the right column. `[` narrows the right column by 10% (panels widen); `]` widens the right column by 10% (panels narrow). The ratio clamps between 30% and 70% for either side. The default ratio is 60% panels / 40% right column. The ratio persists across view changes but resets on app restart.

#### Scenario: Widen right column

- **WHEN** the user presses `]`
- **THEN** the right column widens by 10% and the panels narrow accordingly

#### Scenario: Ratio clamped

- **WHEN** the user adjusts past either bound
- **THEN** the ratio stops at 30% / 70% for either side

### Requirement: Vertical split toggle (quad replacement)

The `\` key SHALL toggle an optional vertical split inside the left panel pane. When active, the left pane splits vertically into two sub-panes: the top shows the hardware component panels, and the bottom shows a secondary view (graph FULL with influence highlight, or graph FILTERED if a modifier is selected). The split ratio within the left pane defaults to 50/50 and is adjustable with `Alt+[`/`Alt+]`. This replaces the permanent quad view (`g q`) from the previous implementation.

#### Scenario: Toggle vertical split

- **WHEN** the user presses `\`
- **THEN** the left pane splits vertically: panels on top, secondary view on bottom

#### Scenario: Vertical split adjusts

- **WHEN** the left pane is split vertically and the user presses `Alt+]`
- **THEN** the bottom sub-pane widens by 10% within the left pane

### Requirement: Narrow-terminal fallback

When the terminal width is below 120 columns, the right column SHALL collapse and only the left panel pane remains visible. A status hint indicates the number of hidden views ("+N views hidden"). The carousel still cycles through hidden views: `Tab` temporarily replaces the left pane with the next view type in the carousel, and `Esc` returns to panels. The split ratio keys (`[`/`]`) have no effect while collapsed.

#### Scenario: Collapse at narrow width

- **WHEN** the terminal is 80 columns wide and two views are open
- **THEN** only the panel pane is visible, and the status bar shows "+2 views hidden"

#### Scenario: Carousel inspects hidden views

- **WHEN** the terminal is narrow and the user presses `Tab`
- **THEN** the left pane temporarily shows the next view type (e.g., graph), and `Esc` returns to panels

### Requirement: Esc closes focused view

`Esc` while a view in the right column is focused SHALL close that view and remove its slot. If the closed view was the only one in the right column, the right column disappears and the left pane takes the full width. If the left panel pane is focused, `Esc` clears any active modifier selection (same as before).

#### Scenario: Close last view in right column

- **WHEN** only the graph view is open in the right column and it is focused, and the user presses `Esc`
- **THEN** the graph view closes, the right column disappears, and the panels take full width

#### Scenario: Close one view, others remain

- **WHEN** graph and source viewer are open in the right column, graph is focused, and the user presses `Esc`
- **THEN** the graph closes and the source viewer expands to fill the right column

### Requirement: Picker and overlays stay on top

The file picker (`l`), validation modal (`e`), label edit overlay, and help modal (`?`) SHALL render as centered overlays on top of the tiled layout, unchanged from their current behavior. They do not become panes in the tiling system.

#### Scenario: Picker overlays tiled layout

- **WHEN** the tiled layout is active with three views and the user presses `l`
- **THEN** the file picker opens as a centered overlay on top of all panes