## REMOVED Requirements

### Requirement: Window lifecycle
**Reason**: The `gui` feature no longer gates an optional window; the window is now the whole application, with no terminal tile fallback and no crossterm state to tear down.
**Migration**: The window lifecycle is described by the `native-rendering` capability.

### Requirement: Multiplexed event loop
**Reason**: The loop polls winit only; crossterm terminal events are gone.
**Migration**: The event loop is owned by the `native-rendering` capability.

### Requirement: Shared scene pipeline
**Reason**: The scene's only consumer is the egui painter; the tiny-skia terminal rasterizer is removed, so the byte-identical terminal-path guarantee no longer applies.
**Migration**: Scene building remains, consumed only by egui.

### Requirement: Two-way live sync
**Reason**: There is no terminal surface to sync with.
**Migration**: Interactions mutate the single app state; the appear-in-the-terminal-same-frame obligation is removed.

## MODIFIED Requirements

### Requirement: Circuit selection propagates across views
Selecting a circuit node in the window SHALL set the shared selection state that the other surfaces reflect: the source viewer SHALL jump to that circuit's section and the panels SHALL highlight the associated hardware. The selection SHALL survive switching between surfaces.

#### Scenario: Select a circuit in the window
- **WHEN** the user clicks a circuit node in the window
- **THEN** the source viewer scrolls to that circuit's section and the panels highlight the associated hardware components

#### Scenario: Selection survives surface switch
- **WHEN** a circuit is selected and the user switches to another surface
- **THEN** the selection remains active in the other views
