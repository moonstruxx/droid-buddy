## Why

DROID patch files (`.ini`) define hardware component configurations for modular synthesizer controllers, but there is no visual tool to inspect, navigate, or interact with patch components in a terminal environment. Users need a lightweight TUI that loads real DROID patches, renders hardware components with clear labels, supports shift-group visualization, and works inside terminal multiplexers like Herdr — enabling patch design and debugging without leaving the terminal.

## What Changes

- Add `.ini` patch file parsing to extract hardware components (buttons, knobs, CV I/O, encoders, LEDs, etc.) from real DROID patch files
- Replace the current sample-patch-only data model with a file-based patch loader
- Add a file picker UI for browsing and selecting `.ini` patch files from disk
- Redesign the main layout from shift-group rows to physical controller panels (P2B8, Faderbank, Notebuttons, etc.) that mirror hardware layout
- Enable mouse capture for clicking buttons, toggling switches, and adjusting knob values
- Add shift-group visualization: when a shift modifier key is held, affected controller panels get a colored border frame while unrelated panels dim
- Handle terminal resize events for multiplexer pane adjustments
- Add Encoder component type to the component model

## Capabilities

### New Capabilities
- `patch-parsing`: Parse DROID `.ini` patch files, extract hardware tokens (B1.1, L1.2, P1.1, O1, I1, E1.1, S1.3), map them to typed hardware components, and build a `Patch` struct from real files
- `file-picker`: Interactive directory browser for selecting `.ini` patch files from disk, with filtering and navigation
- `controller-panels`: Render hardware components grouped by physical controller type (P2B8, Faderbank, Notebuttons, Encoder, etc.) in labeled panels that mirror the physical layout
- `mouse-interaction`: Enable crossterm mouse capture for clicking components to toggle state, hovering for highlight, and scrolling/dragging for knob/fader adjustment
- `shift-visualization`: Visual feedback when shift modifier keys (1-4) are held — affected controller panels get a colored border frame, unrelated panels dim, and the status bar shows the active shift group

### Modified Capabilities
<!-- No existing specs to modify -->

## Impact

- `src/patch.rs`: Extend `ComponentKind` with Encoder, Switch types; add hardware token parsing logic
- `src/app.rs`: Add file picker state, mouse event handling, resize handling
- `src/ui.rs`: Complete layout redesign from shift-group rows to controller panels; add shift visualization with colored borders
- `src/handler.rs`: Add mouse event handling alongside keyboard events
- `src/main.rs`: Enable mouse capture, add resize event handling
- `Cargo.toml`: Add `serde_ini` or `ini` crate for `.ini` parsing
- Existing keyboard navigation (j/k, Enter/Space) preserved alongside new mouse interaction
