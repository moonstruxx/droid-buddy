## 1. Native shell (terminal out)

- [ ] 1.1 Promote the `gui` feature to unconditional, drop `kitty-gfx`, move winit/egui/egui-winit/wgpu to required dependencies, and remove ratatui/crossterm/tiny-skia/fontdue/base64/flate2. Verify `cargo build` succeeds and `cargo tree` no longer lists ratatui or crossterm. <!-- agent: api-engineer.build, depends_on: [], touches: [Cargo.toml, src/lib.rs] -->
- [ ] 1.2 Rewrite main.rs as a native-only winit ApplicationHandler loop; remove the crossterm event drain, terminal init/restore, and the panic-hook terminal teardown; keep config/theme/schema init before the window. Verify the app launches a window and exits cleanly. <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/main.rs] -->
- [ ] 1.3 Delete kitty_protocol.rs and the tiny-skia rasterizer emit path from graph_render.rs; keep GraphCamera and scene building with the egui painter as the only consumer. Verify `cargo build` passes and the graph still draws via egui with no kitty references. <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/kitty_protocol.rs, src/graph_render.rs] -->

## 2. Surface port to egui

- [ ] 2.1 Convert gui.rs into src/gui/mod.rs (window shell + surface dispatch) and move the graph canvas into src/gui/graph.rs. Verify `cargo build` passes and the window opens showing the existing graph canvas. <!-- agent: api-engineer.build, depends_on: [1.2], touches: [src/gui.rs, src/gui/mod.rs, src/gui/graph.rs] -->
- [ ] 2.2 Port the physical 1:1 view to an egui draw routine (mm grid, mm-to-pixel ScreenMapping, pan/zoom, skeleton toggle, element state cells) with pan/zoom/skeleton input. Verify with a shape/label test asserting element cells and pan/zoom math, plus a visual check of the view. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/physical.rs] -->
- [ ] 2.3 Port the controller panels to an egui draw routine (component cells, shift-group borders, module sub-blocks, LED cells) with hover/click/scroll input. Verify with a shape/label test covering shift borders, module sub-blocks, and LED cells. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/panels.rs] -->
- [ ] 2.4 Port the source viewer to an egui draw routine (raw/prettified blocks, sidebar, minimap, occurrence navigation, scroll). Verify with a shape/label test covering both view modes and the sidebar/minimap. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/viewer.rs] -->
- [ ] 2.5 Port the file picker and favourites to an egui overlay. Verify with a shape/label test covering entry ordering and favourites. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/picker.rs] -->
- [ ] 2.6 Port the overlays (validation modal, select-state menu, label editor, diff surface, latency optimizer) to egui. Verify with a shape/label test per overlay. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/overlays.rs] -->

## 3. Input (native)

- [ ] 3.1 Rehost the handler.rs key/mouse semantics onto egui/winit events (shift groups, g-prefix, p/x, scale/split, s/o, Tab focus, Esc, hover/click/scroll/drag) so every binding keeps its meaning. Verify with interaction-mapping tests and by exercising each binding in the window. <!-- agent: rusty-engineer.build, depends_on: [2.2, 2.3, 2.4, 2.5, 2.6], touches: [src/app.rs, src/handler.rs] -->

## 4. Theme

- [ ] 4.1 Extend the Theme::egui_color bridge across the ported surfaces and retire terminal-only tokens. Verify colors resolve for every surface and a theme test passes. <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/theme.rs] -->

## 5. Teardown, tests, docs

- [ ] 5.1 Delete ui.rs (ratatui render paths, TestBackend helpers, box-drawing remnants) once the surfaces no longer reference it. Verify `cargo build` passes with no ratatui imports anywhere. <!-- agent: rusty-engineer.build, depends_on: [2.6, 3.1], touches: [src/ui.rs] -->
- [ ] 5.2 Replace TestBackend snapshot/gallery tests with egui shape+label assertions (headless Context::run -> FullOutput, no GPU); keep all pure-model tests; drop the snapshot-gallery bin. Verify `cargo test` is green with the new assertions and no insta/TestBackend references remain. <!-- agent: horst-engineer.build, depends_on: [5.1], touches: [src/regression.rs, tests/, src/bin/snapshot-gallery.rs] -->
- [ ] 5.3 Regenerate ARCHITECTURE.md and DESIGN.md to describe the native app. Verify the docs no longer reference ratatui/crossterm/terminal rendering. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [5.2], touches: [ARCHITECTURE.md, DESIGN.md] -->

## 6. Verification gate

- [ ] 6.1 Run fmt, clippy (default + gui), cargo test, and release build; all four exit 0. <!-- agent: rusty-engineer.fast, depends_on: [5.3], touches: [] -->
