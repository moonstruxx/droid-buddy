## Why

The terminal is the ceiling. Character-cell resolution, per-frame kitty uploads, and the box-drawing fallback all cap how finely the hardware can be drawn, and the signal-flow graph already outgrew the terminal, which is why the `gui` feature and its egui/wgpu window exist. The native window reaches the fidelity this app needs. This change makes that window the whole app and removes the terminal rendering path.

## What Changes

- **BREAKING:** The app becomes a native egui window. The terminal UI (ratatui + crossterm) is removed. There is no terminal fallback, no headless mode, and no terminal-only build.
- The `gui` feature becomes unconditional. winit, egui, egui-winit, and wgpu move to required dependencies; ratatui, crossterm, tiny-skia, fontdue, base64, and flate2 are removed.
- The physical 1:1 view, controller panels, source viewer, file picker, and every overlay (validation modal, select-state menu, label editor, diff surface, latency optimizer) are reimplemented as egui draw routines over the existing pure models. Patch parsing, the graph model, the layout solver, validation, diff, schema, geometry, and the physical mm model are unchanged.
- Input moves from crossterm key/mouse events to egui/winit events, keeping every existing binding's meaning.
- Visual verification moves from `TestBackend` ANSI/HTML snapshots to egui shape and label assertions.

## Capabilities

### New Capabilities
- `native-rendering`: the native window is the application — its lifecycle, input, resolution, and the native presentation of every surface (physical, panels, viewer, picker, overlays, graph).

### Modified Capabilities
- `gpu-graph-window`: the window is no longer an optional feature-bound second surface syncing with a terminal; it is the only surface.
- `configuration`: the `[gui] graph_window` toggle and its startup seed are removed.
- `render-outlier-detection`: removed — the terminal-width degradation it detects no longer exists.
- `visual-validation`: `TestBackend` ANSI/HTML snapshots are replaced by egui shape and label assertions.
- `tiling-window-manager`: the sub-120-column collapse is removed; panes are pixel-sized.
- `keybinding`: remove `g w` and the narrow-terminal carousel; input events are native.

## Impact

- `Cargo.toml`: dependency and feature change as above.
- `src/main.rs`: native-only winit application loop; config/theme/schema init still runs before the window.
- `src/gui.rs` (becomes the `src/gui/` module): owns the window, per-surface draw routines, and input mapping.
- `src/handler.rs`: key/mouse semantics rehosted onto native events; crossterm types removed.
- `src/ui.rs`, `src/kitty_protocol.rs`: deleted.
- `src/graph_render.rs`: GraphCamera and scene building stay; the tiny-skia rasterizer is removed.
- `src/theme.rs`: egui color bridge extended to all surfaces; terminal-only tokens retired.
- Tests: pure-model tests survive; render/snapshot/gallery tests are replaced.
- Docs: ARCHITECTURE.md and DESIGN.md regenerated.

## Non-goals

- No hardware bridge (MIDI/SysEx), no persistence of component state, no network. These remain out of scope.
- No adoption of eframe or a new app framework; the existing winit/egui/wgpu shell is reused.
- No work on the graph-solver CPU cost (the `droid_tui-3g9` and `droid_tui-oy3` beads). That is a separate optimization track, unaffected by the rendering change.
- No new egui-native engineer agent is created here; work is assigned to the closest-fit existing engineers.
