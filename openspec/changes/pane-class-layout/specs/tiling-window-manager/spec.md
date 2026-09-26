# Tiling Window Manager Specification (Delta)

## REMOVED Requirements

### Requirement: Tiled main band layout
The main band SHALL NOT render as a permanent left panels pane plus a right column of stacked view slots, and SHALL NOT cap visible slots at three with oldest-non-focused eviction.

**Reason**: Superseded by the class-based three-pane layout, where the module UI is a view in the big pane rather than a permanent pane and views are placed by class rather than by slot order.
**Migration**: The module UI starts in the big pane. The graph, physical view, optimizer, and source viewer open through class routing into the pane their class allows.

### Requirement: Carousel rotation
The system SHALL NOT rotate view types through right-column slots with `Tab` and `Shift+Tab`.

**Reason**: Replaced by pane focus cycling, which moves focus across the fixed panes instead of replacing a slot's view.
**Migration**: `Tab` and `Shift+Tab` now cycle focus through the big pane and the two small panes, skipping empty panes. Views open with their own keys (`g g`, `g v`, `g o`) and the physical view keeps its existing entry point.

### Requirement: Focus routing
The system SHALL NOT route focus across a left panel pane and a set of right-column slots.

**Reason**: Replaced by pane focus routing over the fixed three-pane tree.
**Migration**: Focus cycles the big pane and the two small panes in tree order. Focus borders still use `pane_focus_border` and `pane_unfocused_border`.

### Requirement: Split ratio adjustment
The `[` and `]` keys SHALL NOT adjust only the boundary between a left panel pane and a right column.

**Reason**: Replaced by the two pane boundaries of the class layout, with the optimizer's weight binding taking precedence while it is focused.
**Migration**: `[`/`]` move the big-pane boundary and `Alt+[`/`Alt+]` move the small-pane boundary, both clamped to 30 and 70 percent.

### Requirement: Vertical split toggle (quad replacement)
The `\` key SHALL NOT toggle a vertical split inside a left panel pane.

**Reason**: The left panel pane no longer exists, so there is nothing to sub-split. The two small panes already provide a stacked arrangement.
**Migration**: `\` is unbound. Use `Alt+]`/`Alt+[` to resize the two small panes, or `z` to maximize one pane.

### Requirement: Esc closes focused view
`Esc` SHALL NOT close a view and remove its slot from a right column.

**Reason**: Slots no longer exist. Panes persist and hold at most one view each.
**Migration**: `Esc` closes the focused pane's view and leaves the pane empty. When the focused pane is maximized, `Esc` clears the maximize first.

### Requirement: Picker and overlays stay on top
The requirement that the picker, validation modal, label overlay, and help modal render as centered overlays above the tiled layout SHALL NOT be carried by this capability.

**Reason**: The requirement is not specific to the tiling layout and moves with the base surface.
**Migration**: The same overlays still render above the pane layout. `pane-class-layout` now states that requirement.
