## 1. Native shell (egui first-class, terminal still compiles)

- [x] 1.1 Promote the `gui` feature to unconditional, drop the `kitty-gfx` feature, and make winit/egui/egui-winit/wgpu required dependencies. Keep ratatui/crossterm/tiny-skia/fontdue/base64/flate2 in place until the teardown (3.1) removes them. Verify `cargo build` succeeds with egui and the terminal both present. <!-- agent: api-engineer.build, depends_on: [], touches: [Cargo.toml, src/lib.rs] -->
- [x] 1.2 Split gui.rs into src/gui/mod.rs (window shell + surface dispatch) and move the graph canvas into src/gui/graph.rs. Verify `cargo build` passes and the window opens showing the existing graph canvas. <!-- agent: api-engineer.build, depends_on: [1.1], touches: [src/gui.rs, src/gui/mod.rs, src/gui/graph.rs] -->

## 2. Surface port to egui

- [x] 2.1 Port the physical 1:1 view to an egui draw routine (mm grid, mm-to-pixel ScreenMapping, pan/zoom, skeleton toggle, element state cells) with pan/zoom/skeleton input. Verify with a shape/label test asserting element cells and pan/zoom math. <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [src/gui/physical.rs] -->
- [ ] 2.2 Port the controller panels to an egui draw routine (component cells, shift-group borders, module sub-blocks, LED cells) with hover/click/scroll input. Verify with a shape/label test covering shift borders, module sub-blocks, and LED cells. <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [src/gui/panels.rs] -->
- [ ] 2.3 Port the source viewer to an egui draw routine (raw/prettified blocks, sidebar, minimap, occurrence navigation, scroll). Verify with a shape/label test covering both view modes and the sidebar/minimap. <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [src/gui/viewer.rs] -->
- [ ] 2.4 Port the file picker and favourites to an egui overlay. Verify with a shape/label test covering entry ordering and favourites. <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [src/gui/picker.rs] -->
- [ ] 2.5 Port the overlays (validation modal, select-state menu, label editor, diff surface, latency optimizer) to egui. Verify with a shape/label test per overlay. <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [src/gui/overlays.rs] -->
- [ ] 2.6 Rehost the handler.rs key/mouse semantics onto egui/winit events (shift groups, g-prefix, p/x, scale/split, s/o, Tab focus, Esc, hover/click/scroll/drag) so every binding keeps its meaning. Verify with interaction-mapping tests. <!-- agent: rusty-engineer.build, depends_on: [2.1, 2.2, 2.3, 2.4, 2.5], touches: [src/app.rs, src/handler.rs] -->
- [ ] 2.7 Extend the Theme::egui_color bridge across the ported surfaces and retire terminal-only tokens. Verify colors resolve for every surface and a theme test passes. <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [src/theme.rs] -->

## 3. Teardown (single red-to-green flip)

- [ ] 3.1 Make main.rs native-only (drop the crossterm drain), delete kitty_protocol.rs and the tiny-skia emit path, delete ui.rs, and remove ratatui/crossterm/tiny-skia/fontdue/base64/flate2 from Cargo.toml. Verify `cargo build` is green with no ratatui or crossterm references anywhere. <!-- agent: rusty-engineer.build, depends_on: [2.6, 2.7], touches: [src/main.rs, src/kitty_protocol.rs, src/graph_render.rs, src/ui.rs, Cargo.toml] -->

## 4. Tests and docs

- [ ] 4.1 Replace TestBackend snapshot/gallery tests with egui shape+label assertions (headless Context::run -> FullOutput, no GPU); keep all pure-model tests; drop the snapshot-gallery bin. Verify `cargo test` is green with no insta/TestBackend references. <!-- agent: horst-engineer.build, depends_on: [3.1], touches: [src/regression.rs, tests/, src/bin/snapshot-gallery.rs] -->
- [ ] 4.2 Regenerate ARCHITECTURE.md and DESIGN.md to describe the native app. Verify the docs no longer reference ratatui/crossterm/terminal rendering. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [4.1], touches: [ARCHITECTURE.md, DESIGN.md] -->

## 5. Verification gate

- [ ] 5.1 Run fmt, clippy (default + gui), cargo test, and release build; all four exit 0. <!-- agent: rusty-engineer.fast, depends_on: [4.2], touches: [] -->
