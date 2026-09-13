## REMOVED Requirements

### Requirement: Snapshot generation from TestBackend
**Reason**: The terminal is removed; `TestBackend` ANSI snapshots and the HTML gallery no longer have a rendering backend.
**Migration**: Replace with egui shape and label assertions (see the ADDED requirement below).

### Requirement: Start-small coverage matrix
**Reason**: The matrix scenarios were terminal face cases (panel cells, boxed LED borders, fader tracks, zoom-preset cell overlap) rendered through `TestBackend`.
**Migration**: Equivalent scenarios move to egui shape/label assertions under the `native-rendering` capability.

### Requirement: Strict gate on face regression
**Reason**: The insta ANSI snapshot gate is tied to `TestBackend`.
**Migration**: The regression gate becomes the egui shape/label assertions.

### Requirement: Ephemeral worktree, durable archive
**Reason**: The ANSI/HTML gallery artifact is gone with the terminal.
**Migration**: Delete the gallery archive step.

### Requirement: Insta-managed golden
**Reason**: insta golden files are tied to `TestBackend` ANSI output.
**Migration**: Remove insta from the rendering assertions.

### Requirement: Tiled layout coverage
**Reason**: These gallery rows rendered the column-based tiling at character widths, which no longer exists.
**Migration**: The tiled layout is covered by native pane content assertions.

### Requirement: Focus state coverage
**Reason**: Focus-border snapshots were terminal rows; focus is now a native pane state.
**Migration**: Cover focus with native pane state assertions.

### Requirement: Optimizer pane coverage
**Reason**: The optimizer pane gallery row was terminal-rendered.
**Migration**: Cover the optimizer overlay with egui shape/label assertions.

### Requirement: Skeleton and full proof rows
**Reason**: The side-by-side gallery proof used terminal frames.
**Migration**: The skeleton/full coincidence assertion survives as a pure-model check, independent of rendering.

### Requirement: Plugin-circuit coverage row
**Reason**: The plugin-circuit gallery row was terminal-rendered.
**Migration**: Cover the plugin-circuit declared kind/color with egui shape/label assertions.

## ADDED Requirements

### Requirement: Egui shape and label assertions
The system SHALL verify each surface's rendering by driving a headless egui context and asserting on the produced shapes and labels (element cells, borders, titles, state glyphs), without opening a window or GPU.

#### Scenario: Surface render is asserted headlessly
- **WHEN** a surface renders into a headless egui context
- **THEN** a test asserts the expected shapes and labels appear in the output
