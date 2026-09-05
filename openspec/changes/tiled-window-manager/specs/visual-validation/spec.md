## ADDED Requirements

### Requirement: Tiled layout coverage

The visual-validation matrix SHALL include scenarios for the tiled window manager layout: single view in right column, two views split horizontally, three views split horizontally, and narrow-terminal collapse (below 120 columns). Each scenario SHALL render under `classic`, `mono`, and `terminal` themes at widths 80, 120, and 160 columns.

#### Scenario: Single view tiled
- **WHEN** the gallery renders a patch with the graph view open in the right column
- **THEN** a snapshot exists showing panels on the left and graph on the right at the configured split ratio

#### Scenario: Two views tiled
- **WHEN** the gallery renders a patch with graph and source viewer open in the right column
- **THEN** a snapshot exists showing panels on the left and two horizontally stacked views on the right

#### Scenario: Narrow terminal collapse
- **WHEN** the gallery renders at 80 columns with multiple views open
- **THEN** a snapshot exists showing only the panel pane with a "+N views hidden" status hint

### Requirement: Focus state coverage

The gallery SHALL include scenarios showing the focus border on each pane type (panels, graph, source viewer) under each theme. The focused pane's border SHALL use the `pane_focus_border` token; unfocused panes SHALL use `pane_unfocused_border`.

#### Scenario: Focus border on graph pane
- **WHEN** the graph pane is focused in a tiled layout
- **THEN** the snapshot shows the graph pane with the focus border token and other panes with the unfocused token

### Requirement: Optimizer pane coverage

The gallery SHALL include scenarios for the optimizer as a side pane, showing candidate listings alongside the visible panel view.

#### Scenario: Optimizer pane with panels
- **WHEN** the optimizer pane is open via `g o`
- **THEN** a snapshot exists showing the optimizer pane in the right column with panel content visible in the left pane

## REMOVED Requirements

### Requirement: Quad-view coverage
**Reason**: The permanent quad view (`g q`) is replaced by the tiling window manager's optional vertical split (`\`).
**Migration**: Gallery coverage for side-by-side views uses the tiled layout scenarios with the vertical split toggle active.
