## MODIFIED Requirements

### Requirement: View-switch key

The system SHALL provide a `g g` key sequence to open the signal-flow graph view as a slot in the right column of the tiled layout. `Esc` closes the graph view and removes its slot from the right column.

#### Scenario: Open graph with g g
- **WHEN** in normal mode with a patch loaded, the user presses `g` then `g`
- **THEN** the graph view opens as a slot in the right column alongside the controller panels

#### Scenario: Close graph with Esc
- **WHEN** the graph view is open and the user presses `Esc`
- **THEN** the graph view closes, its slot is removed from the right column, and any component selection is preserved

## ADDED Requirements

### Requirement: Graph camera zoom via focus

The graph pane SHALL respond to `+`/`-` for camera zoom when it has focus. `Shift++`/`Shift+-` SHALL adjust zoom regardless of which pane is focused. The existing mouse-wheel zoom behavior remains unchanged.

#### Scenario: Zoom focused graph
- **WHEN** the graph pane is focused and the user presses `+`
- **THEN** the graph camera zooms in and the graph image re-transmits at the new scale

#### Scenario: Zoom other pane
- **WHEN** the panel pane is focused and the user presses `Shift++`
- **THEN** the graph camera zooms in (if graph is open in the right column)

### Requirement: Cable tension keys

`Alt+[`/`Alt+]` SHALL lower/raise the graph's cable tension (solver spring stiffness) when the graph pane has focus, re-solving the layout live.

#### Scenario: Adjust cable tension
- **WHEN** the graph pane is focused and the user presses `Alt+]`
- **THEN** the cable tension increases by 0.05 and the layout re-solves
