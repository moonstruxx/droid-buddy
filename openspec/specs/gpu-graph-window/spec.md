# gpu-graph-window Specification

## Purpose

A GPU-accelerated, Comfy-quality graph window for the signal-flow graph, bound to droid_tui in one process with full two-way live sync. The window renders the same graph model, solver positions, camera, and color tokens as the terminal tile, and window interactions mutate the same application state.

## Requirements

### Requirement: Window lifecycle

The `gui` Cargo feature SHALL gate the GPU window. With the feature enabled, `g w` SHALL open and close the GPU graph window, and `Esc` SHALL close it. The window SHALL be a normal desktop window owned by the droid_tui process. Quitting the app SHALL tear down both the window and the raw terminal state. The window SHALL never open during `cargo test` or in headless environments; the terminal graph tile remains the fallback.

#### Scenario: Open and close via g w

- **WHEN** the app runs with the `gui` feature and the user presses `g` then `w`
- **THEN** the GPU window opens showing the signal-flow graph, and pressing `g w` again (or `Esc`) closes it

#### Scenario: No window in tests

- **WHEN** `cargo test` runs with the `gui` feature enabled
- **THEN** no window is created and all graph tests exercise the scene pipeline directly

### Requirement: Multiplexed event loop

The app SHALL run a single-threaded event loop that polls both window events (winit) and terminal events (crossterm) each frame, with both event sources mutating the same `App` state. No IPC, no second process, no async runtime.

#### Scenario: One thread owns all state

- **WHEN** the user drags a node in the window and presses a key in the terminal in the same frame
- **THEN** both inputs mutate the same `App` state without locking, and the next frame of both surfaces reflects both changes

### Requirement: Shared scene pipeline

The graph scene builder SHALL produce a backend-neutral description (nodes, ports, edges with per-cable colors, cluster containers, labels) consumed by both the tiny-skia terminal rasterizer and the egui window painter. The terminal rendering path SHALL remain byte-identical.

#### Scenario: Identical scene, two backends

- **WHEN** a patch's graph is rendered in both the terminal tile and the GPU window
- **THEN** both consume the same scene description and camera, so node positions, edges, and colors match

### Requirement: Two-way live sync

Window interactions SHALL map onto the existing application mutations: dragging a node SHALL re-settle locally and emit `NodeMoved`; `x`, `p`, and `e` in the window SHALL toggle processing, pin/unpin, and edit labels exactly as in the terminal. State changes made in the window SHALL appear in the terminal the same frame, and vice versa.

#### Scenario: Drag in window moves terminal state

- **WHEN** the user drags a node in the GPU window
- **THEN** the node's position updates in shared state, the local neighborhood re-settles, and the terminal tile shows the node at the new position

#### Scenario: Disable circuit from window

- **WHEN** the user presses `x` on a hovered node in the window
- **THEN** that circuit instance is disabled, the graph rebuilds, and both surfaces render the node and its edges dimmed

### Requirement: Circuit selection propagates across views

Selecting a circuit node in the GPU window SHALL set the shared selection state that the other surfaces reflect: the source viewer SHALL jump to that circuit's section, the panels SHALL highlight the associated hardware, and an open terminal graph tile SHALL highlight the same node. The selection SHALL survive switching between surfaces.

#### Scenario: Select a circuit in the window

- **WHEN** the user clicks a circuit node in the GPU window
- **THEN** the source viewer scrolls to that circuit's section, the panels highlight the associated hardware components, and a terminal graph tile highlights the same node

#### Scenario: Selection survives surface switch

- **WHEN** a circuit is selected in the window and the user closes the window
- **THEN** the selection remains active in the terminal views

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