## Why

The terminal-based graph surface (box-drawing cells and the kitty rasterizer) cannot reach the smoothness and fidelity of a Comfy-style node graph: GPU-accelerated canvas, fluid pan and zoom, marquee selection, minimap, direct manipulation. Every terminal path is bounded by cell resolution or per-frame protocol uploads. The graph view needs a real GPU-accelerated window bound to droid_tui, with full two-way live sync between window and terminal.

## What Changes

- **GPU graph window.** A winit/egui window with a wgpu backend renders the signal-flow graph on a GPU-accelerated 2D canvas. Opened with `g w`, or with `g g` when `[gui] graph_window = true` is set.
- **One process, one thread.** The winit event loop becomes the main loop and polls crossterm events each frame; window and terminal events both mutate the same `App` state. No IPC, no async runtime, no second process.
- **Full two-way live sync.** Dragging a node, toggling processing (`x`), pinning (`p`), and editing labels (`e`) in the window behave identically to the terminal surface: a drag re-settles locally and emits `NodeMoved`, and every state change appears in the terminal the same frame.
- **Circuit selection propagates across views.** Selecting a circuit in the GPU window sets the shared selection, which the other surfaces reflect: the source viewer jumps to the circuit's section, the panels highlight the associated hardware, and an open terminal graph tile highlights the same node.
- **Shared scene pipeline.** `graph_render.rs` scene building (node frames, ports, bezier cables with per-cable colors, cluster containers, labels) becomes backend-neutral and is consumed by both the tiny-skia terminal rasterizer and the egui painter. The terminal path stays byte-identical.
- **Canvas polish.** Smooth pan and zoom under the existing `GraphCamera`, marquee selection, minimap, hover tooltip with latency readouts.
- **Theme bridge.** The window derives all colors from the existing semantic theme tokens, keeping palettes consistent between terminal and window.
- **Feature-gated.** `gui` is a non-default feature; `--no-default-features` builds and the default terminal-only experience are unchanged.

## Capabilities

### New Capabilities
- `gpu-graph-window`: window lifecycle, multiplexed event loop, shared scene pipeline, egui canvas rendering, two-way interaction, and canvas polish (pan/zoom, marquee selection, minimap, tooltips).

### Modified Capabilities
- `signal-flow-graph`: graph interaction semantics are shared with the GPU window; the surface may render in a window or a terminal tile with identical behavior.
- `keybinding`: `g w` opens and closes the GPU window; `g g` opens the window instead of the tile when configured.
- `configuration`: new `[gui]` section with the `graph_window` toggle.

## Impact

- `Cargo.toml` — optional `gui` feature; winit, egui, egui-winit, and wgpu dependencies behind it.
- `src/main.rs` — event loop restructured into a winit application handler that polls crossterm events; the panic hook tears down both the window and the terminal.
- `src/gui.rs` — new module owning the window, the egui canvas, and window input mapping.
- `src/graph_render.rs` — scene building extracted to a backend-neutral spec consumed by tiny-skia and egui.
- `src/app.rs` / `src/handler.rs` — shared interaction entry points for window events; the `g w` binding.
- `src/config.rs` — `[gui]` section load and save.
- `src/theme.rs` — egui color mapping over the existing semantic tokens.
- Tests — scene-spec unit tests; no window opens under `cargo test`.