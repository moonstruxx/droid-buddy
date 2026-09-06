# Design: gpu-graph-window

## Context

The signal-flow graph currently renders in the terminal through box-drawing cells or the kitty rasterizer (tiny-skia + fontdue). Both paths are bounded by cell resolution or per-frame protocol uploads, so they cannot reach the smoothness of a Comfy-style node graph: GPU-accelerated canvas, fluid pan and zoom, marquee selection, minimap, direct manipulation.

Exploration with the user settled the direction: a GPU-accelerated 2D window (Comfy quality bar, not literal 3D) bound to droid_tui in one process, with full two-way live sync. Circuit selection in the graph window must propagate to the other views (panels, source viewer, terminal graph tile).

The codebase is a single-crate, single-threaded, no-async, no-network layered monolith. The graph model (`graph.rs`), deterministic solver (`layout.rs`), camera (`GraphCamera`), and theme tokens are terminal-independent and reusable as-is. The scene builder in `graph_render.rs` already produces colored node/edge specs for the kitty rasterizer.

## Goals / Non-Goals

### Goals

- A GPU-accelerated graph window (egui + wgpu) that renders the same graph as the terminal tile, from the same model, solver positions, camera, and theme tokens.
- Full two-way live sync: window drags, `x`, `p`, `e` map onto the existing App mutations; state changes appear in both surfaces the same frame.
- Circuit selection in the window propagates to the panels, source viewer, and terminal graph tile.
- One process, one thread, multiplexed event loop; no IPC, no async runtime, no second binary.
- `gui` as a non-default Cargo feature; default and `--no-default-features` builds unchanged; terminal graph tile and kitty raster stay as fallback.
- Canvas polish: smooth pan/zoom, marquee selection, minimap, hover tooltip with latency readouts.

### Non-Goals

- Literal 3D rendering; the window is GPU-accelerated 2D.
- A second process or IPC channel; all state lives in one thread.
- A webview or localhost server (Tauri/React Flow route deferred unless the polish bar becomes non-negotiable).
- Herdr-specific integration; herdr is a terminal multiplexer and cannot host GPU windows. The window is a desktop window launched from the TUI running in a herdr pane; herdr needs no changes.
- Window layout persistence across sessions.
- `egui_graphs` integration; the canvas is hand-rolled on egui to preserve per-cable coloring, clusters, and the latency ramp.

## Decisions

### D1: One process, one thread, multiplexed event loop

winit requires the main thread on Linux (especially Wayland). The winit application handler with `ControlFlow::Poll` becomes the main loop; each frame drains crossterm events via `event::poll(Duration::ZERO)` and window events via winit, both mutating the same `App`. No Mutex is needed because one thread owns everything. The panic hook must tear down both the window and the raw terminal state.

Rationale: full two-way live sync at frame rate rules out file or socket channels (they cannot mirror drags, hover, and diff scope live). The multiplexed loop keeps the repo's single-threaded, no-async property.

### D2: wgpu backend behind a `gui` feature

egui paints through wgpu (Vulkan/Metal/DX12), the current standard choice. The `gui` feature is non-default: `--no-default-features` and the default terminal-only build stay lean, and CI compiles them without the wgpu dependency tree. Headless environments keep the terminal tile and kitty raster as fallback.

### D3: Shared scene spec over two painters

Refactor `graph_render.rs` so scene building (node frames, ports, per-cable colored edges, cluster containers, labels) emits a backend-neutral description. tiny-skia (terminal) and egui (window) consume the same specs and camera; the terminal path stays byte-identical. Hit-testing keeps deriving from the same camera both surfaces use.

### D4: Two-way sync boundary reuses existing mutations

Window events map onto existing App entry points: drag -> `local_resettle` + `NodeMoved`, `x` -> `toggle_circuit_processing`, `p` -> pin toggle, `e` -> label overlay. Circuit selection is new shared state set by clicking a node in either surface; the source viewer jumps to the circuit's section, panels highlight the associated hardware, and the terminal tile highlights the same node. This is the only new App state the change introduces.

### D5: Hand-rolled egui canvas

The window paints directly with egui `Painter` (rounded rects, bezier strokes, text) from the shared scene spec, instead of adopting `egui_graphs`. The existing per-cable precedence (error red > diff > latency ramp > cable kind > dim) and cluster containers map 1:1. Marquee selection and minimap are implemented as canvas features on top.

### D6: Keybinding and config

`g w` opens and closes the window (feature-gated; status hint without the feature). `[gui] graph_window = true` makes `g g` open the window instead of the terminal tile; default `false`. Config load/save follows the existing `config.rs` patterns (warn-once on malformed values).

### D7: Theme bridge

All window colors derive from the existing semantic theme tokens via the same `theme::rgb` mapping the kitty path uses, converted to egui `Color32`. No hardcoded RGB in the window painter.

## Risks / Trade-offs

- **Dependency weight**: wgpu is a large, slow-to-compile dependency tree; the non-default `gui` feature contains it. First build with the feature will be long.
- **Wayland main-thread constraint**: the multiplexed loop is built around winit owning the main thread; the terminal loop becomes a polled source. Input latency stays within a frame because `ControlFlow::Poll` drives redraws on demand.
- **Headless/SSH environments**: no display means no window; the terminal tile and kitty raster remain the fallback, so the graph stays usable everywhere.
- **Two event sources, one loop**: crossterm mouse capture and winit mouse input are independent surfaces, so they do not conflict. Determinism of the solver is preserved because the window uses the same solver, seed, and camera.
- **Testing**: snapshot tests cannot capture a GPU window. Unit tests exercise the scene spec and the interaction mapping directly (no window is created under `cargo test`); window rendering is verified manually or via screenshots.