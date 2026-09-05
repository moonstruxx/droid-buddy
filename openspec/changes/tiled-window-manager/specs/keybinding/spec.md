## MODIFIED Requirements

### Requirement: Split ratio keys

When any view is open in the right column, the `[` and `]` keys SHALL adjust the horizontal split ratio between the left panel pane and the right column.

#### Scenario: Widen right column
- **WHEN** the user presses `]`
- **THEN** the right column widens by 10% and the panels column shrinks accordingly

#### Scenario: Narrow right column
- **WHEN** the user presses `[`
- **THEN** the right column narrows by 10% and the panels column widens accordingly

#### Scenario: Ratio clamped
- **WHEN** the user adjusts past either bound
- **THEN** the ratio stops at 30% / 70% for either side

#### Scenario: No right column
- **WHEN** no view is open in the right column
- **THEN** pressing `[` or `]` has no effect on layout state

## REMOVED Requirements

### Requirement: Split ratio keys (viewer-only)
**Reason**: Replaced by the tiling window manager's split ratio keys, which apply to any right-column view, not just the source viewer.
**Migration**: The `[`/`]` keys now adjust the left/right split regardless of which view occupies the right column. The source viewer no longer has exclusive split control.

### Requirement: Focus switching key
**Reason**: Replaced by the tiling window manager's focus routing, which cycles across all visible panes.
**Migration**: `Tab` now cycles focus across all panes in the tiled layout (left panel pane, then each right-column slot top-to-bottom).

### Requirement: Viewer close
**Reason**: Replaced by the tiling window manager's `Esc` closes focused view behavior.
**Migration**: `Esc` closes the focused view and removes its slot from the right column.

### Requirement: Mouse click routes focus
**Reason**: Replaced by the tiling window manager's mouse click sets focus behavior.
**Migration**: Mouse clicks on any pane set focus to that pane, consistent with the tiling focus routing.

## ADDED Requirements

### Requirement: Carousel rotation keys

The system SHALL provide `Tab` and `Shift+Tab` to cycle views through the right-column slots. `Tab` cycles forward (graph → source viewer → physical → graph...); `Shift+Tab` cycles backward.

#### Scenario: Tab cycles forward
- **WHEN** the right column contains the graph view and the user presses `Tab`
- **THEN** the graph slot is replaced with the source viewer

#### Scenario: Shift+Tab cycles backward
- **WHEN** the right column contains the source viewer and the user presses `Shift+Tab`
- **THEN** the source viewer slot is replaced with the graph view

### Requirement: Vertical split toggle

The `\` key SHALL toggle an optional vertical split inside the left panel pane, replacing the quad view (`g q`) binding.

#### Scenario: Toggle vertical split
- **WHEN** the user presses `\`
- **THEN** the left pane splits vertically into panels (top) and a secondary view (bottom)

### Requirement: Narrow-terminal carousel

When the terminal width is below 120 columns and the right column is collapsed, `Tab` SHALL temporarily replace the left panel pane with the next view in the carousel, and `Esc` SHALL return to panels.

#### Scenario: Inspect hidden view
- **WHEN** the terminal is narrow and the user presses `Tab`
- **THEN** the left pane temporarily shows the next view type, and `Esc` returns to panels

### Requirement: Zoom family keys

The system SHALL consolidate zoom-like actions onto `+`/`-` with modifier variants:
- `+`/`-` (no modifier): scale the focused pane (panels scale when panels focused, graph camera zoom when graph focused).
- `Shift++`/`Shift+-`: scale the other (non-focused) pane.
- `Alt+[`/`Alt+]`: adjust cable tension when graph has focus.

#### Scenario: Scale focused pane
- **WHEN** the graph pane is focused and the user presses `+`
- **THEN** the graph camera zooms in

#### Scenario: Scale other pane
- **WHEN** the graph pane is focused and the user presses `Shift++`
- **THEN** the panel pane scale increases
