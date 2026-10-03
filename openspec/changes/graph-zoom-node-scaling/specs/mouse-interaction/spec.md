# Spec Delta

## ADDED Requirements

### Requirement: Graph node hit-testing follows zoomed geometry

While the graph pane is focused, pointer hit-testing SHALL test the same world-space node extents the renderer draws, independent of the camera zoom, and SHALL apply a minimum pixel hit size so a node remains selectable when its drawn frame is only a few pixels.

#### Scenario: Click a node at low zoom

- **WHEN** the camera is zoomed out so a node's drawn frame is a few pixels wide and the user clicks its center
- **THEN** the node becomes hovered and selected.

#### Scenario: Click a node at high zoom

- **WHEN** the camera is zoomed in and the user clicks inside a node's frame
- **THEN** the node is selected; a click in the gap between two node frames selects nothing.
