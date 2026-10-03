# Spec Delta

## Purpose

Lets users navigate the signal-flow graph by clicking on the minimap panel, instantly panning the camera to the clicked world position.

## ADDED Requirements

### Requirement: Minimap click pans camera

The system SHALL pan the graph camera when the user left-clicks on the graph minimap panel. The clicked point in minimap space SHALL map to a world position using the minimap's world-to-minimap scale factors, and the camera SHALL center its viewport on that world position.

#### Scenario: Click minimap centers camera on clicked location
- **WHEN** graph pane is open and minimap is visible, AND user left-clicks inside the minimap panel
- **THEN** graph camera pans so the clicked minimap position maps to the world center of the visible viewport

#### Scenario: Click outside minimap does not trigger navigation
- **WHEN** user left-clicks on the graph canvas but outside the minimap panel
- **THEN** normal graph interaction applies (node drag, selection, marquee) without camera jump

#### Scenario: Minimap click works at any zoom level
- **WHEN** graph is zoomed in or out, AND user clicks minimap
- **THEN** camera pans to the correct world position regardless of current zoom

### Requirement: Minimap rect published for hit-testing

The renderer SHALL publish the minimap panel's screen rectangle in terminal cell coordinates to `App.graph_minimap_rect` each frame when the minimap is drawn, so the handler can hit-test mouse clicks against it.

#### Scenario: Minimap rect available in handler
- **WHEN** graph pane renders with minimap visible
- **THEN** `App.graph_minimap_rect` contains the panel's terminal-cell rect matching the drawn minimap

### Requirement: Help documents minimap click

The help modal SHALL list "click minimap" as a navigation method in the Signal-flow Graph view.

#### Scenario: Help shows minimap click
- **WHEN** user opens help (`?`) while graph pane is focused
- **THEN** help text includes "click minimap" or equivalent description for navigation