## 1. Window scaffold

- [x] 1.1 Add optional `gui` feature; winit/egui/egui-winit/wgpu deps; `src/gui.rs` window skeleton <!-- agent: api-engineer.build, depends_on: [], touches: [Cargo.toml, src/lib.rs, src/gui.rs] -->
- [x] 1.2 Multiplexed event loop: winit ApplicationHandler + polled crossterm per frame; panic hook tears down window and terminal <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/main.rs, src/gui.rs] -->

## 2. Shared scene pipeline

- [x] 2.1 Refactor graph_render.rs scene building into a backend-neutral spec; tiny-skia path stays byte-identical <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/graph_render.rs] -->
- [x] 2.2 egui painter backend drawing the same spec (node frames, ports, bezier cables, clusters) under GraphCamera <!-- agent: api-engineer.build, depends_on: [2.1], touches: [src/gui.rs, src/graph_render.rs] -->

## 3. Two-way interaction

- [x] 3.1 Map window events to existing App mutations: drag -> local_resettle + NodeMoved, hover, x/p/e; single-thread shared state <!-- agent: rusty-engineer.build, depends_on: [1.2, 2.2], touches: [src/app.rs, src/handler.rs, src/gui.rs] -->
- [x] 3.2 Canvas polish: smooth pan/zoom, marquee selection, minimap, hover tooltip with latency readouts <!-- agent: api-engineer.build, depends_on: [2.2, 3.1], touches: [src/gui.rs] -->
- [x] 3.3 Circuit selection propagation: selecting a circuit in the window or terminal tile sets shared selection; source viewer jumps to the section, panels highlight hardware, terminal tile highlights the node <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/app.rs, src/handler.rs, src/ui.rs] -->

## 4. Keybinding and config

- [x] 4.1 `g w` toggles the window; `[gui] graph_window = true` makes `g g` open the window; config load/save <!-- agent: rusty-engineer.fast, depends_on: [3.1], touches: [src/handler.rs, src/config.rs] -->

## 5. Theme bridge

- [ ] 5.1 Map theme semantic tokens to egui colors; window styling consistent with terminal palettes <!-- agent: layout-designer-engineer.build, depends_on: [2.2], touches: [src/theme.rs, src/gui.rs] -->

## 6. Tests

- [ ] 6.1 Unit tests: scene-spec pipeline, interaction mapping, selection propagation, config parse; no window opens under cargo test <!-- agent: horst-engineer.build, depends_on: [2.1, 3.1, 3.3, 4.1], touches: [src/gui.rs, src/config.rs, src/app.rs] -->

## 7. Verification gate

- [ ] 7.1 fmt, clippy all-features, test (default + gui), release build <!-- agent: rusty-engineer.fast, depends_on: [6.1], touches: [] -->