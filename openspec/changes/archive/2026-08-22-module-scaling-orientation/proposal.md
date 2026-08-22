## Why

The DROID patch viewer currently displays components at fixed sizes without considering module scaling or orientation changes. When patches are loaded from different hardware configurations or when users want to visualize how components reflow under different display conditions, there is no mechanism to scale or reorient the component layout. This limits the viewer's adaptability to different use cases and hardware revisions.

## What Changes

- Add module scaling support to allow components to be rendered at configurable scale factors
- Add orientation support (portrait/landscape) for component panel layout
- Update the renderer to handle scale and orientation transformations
- Add configuration UI for users to adjust scaling and orientation settings

## Capabilities

### New Capabilities

- `module-scaling`: Support for configurable component scaling with preset ratios (50%, 100%, 150%, 200%)
- `module-orientation`: Support for switching between portrait and landscape panel arrangement

### Modified Capabilities

- None - all capabilities are new additions

## Impact

- **Renderer (ui.rs)**: Add scale factor and orientation state to rendering logic; compute component geometry with transformation matrices
- **App state (app.rs)**: Add `scale_factor: f32` and `orientation: Orientation` fields; update picker and status bar to reflect current mode
- **Handler (handler.rs)**: Add key bindings for scaling (`+`, `-`) and orientation switching (`o`), plus mouse wheel scaling support
- **Patch model (patch.rs)**: No changes needed - scaling/orientation are rendering-time concerns
- **Dependencies**: No external dependencies added

### Non-Goals

- Persisting scale/orientation state across sessions
- Hardware bridge integration (MIDI, network)
- Adding new component kinds or modifying existing ones
- Changing the .ini patch format