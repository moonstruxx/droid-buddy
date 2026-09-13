# gpu-graph-window Specification

## Purpose

A GPU-accelerated, Comfy-quality graph window for the signal-flow graph, bound to droid_tui in one process with full two-way live sync. The window renders the same graph model, solver positions, camera, and color tokens as the terminal tile, and window interactions mutate the same application state.

## Requirements

### Requirement: Circuit selection propagates across views
Selecting a circuit node in the window SHALL set the shared selection state that the other surfaces reflect: the source viewer SHALL jump to that circuit's section and the panels SHALL highlight the associated hardware. The selection SHALL survive switching between surfaces.

#### Scenario: Select a circuit in the window
- **WHEN** the user clicks a circuit node in the window
- **THEN** the source viewer scrolls to that circuit's section and the panels highlight the associated hardware components

#### Scenario: Selection survives surface switch
- **WHEN** a circuit is selected and the user switches to another surface
- **THEN** the selection remains active in the other views

### Requirement: Canvas polish

The GPU window SHALL provide smooth pan and zoom under the shared `GraphCamera`, marquee selection over multiple nodes, a minimap of the full layout, and a hover tooltip showing the circuit's latency readouts.

#### Scenario: Pan and zoom the canvas

- **WHEN** the user scrolls or drags the window canvas
- **THEN** the view pans and zooms smoothly and hit-testing stays aligned with the drawn geometry

#### Scenario: Marquee selection

- **WHEN** the user drags a selection rectangle across several nodes
- **THEN** all enclosed nodes become selected and the selection propagates to the other views

#### Scenario: Minimap

- **WHEN** the graph is larger than the window viewport
- **THEN** a minimap shows the full layout with a viewport indicator

### Requirement: Theme derivation

The window SHALL derive every node, edge, label, and background color from the active theme's semantic tokens, never from hardcoded RGB values, keeping palettes consistent between terminal and window.

#### Scenario: Window matches terminal theme

- **WHEN** the active theme is changed
- **THEN** both the terminal and the window reflect the same semantic colors
