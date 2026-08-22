## 1. App State Changes

- [ ] 1.1 Add `scale_factor: f32` field to `App` struct in `src/app.rs` with default 1.0 <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [ ] 1.2 Add `orientation: Orientation` field to `App` struct in `src/app.rs` with default `Portrait` <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [ ] 1.3 Add `Orientation` enum definition with `Portrait` and `Landscape` variants in `src/app.rs` <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [ ] 1.4 Implement `refresh_picker_entries` update to account for scale/orientation if needed <!-- agent: rusty-engineer.build, depends_on: [1.1, 1.2, 1.3], touches: [src/app.rs] -->
- [ ] 1.5 Write unit test for `App::new` with scale/orientation defaults and verify checkbox <!-- agent: horst-engineer.fast, depends_on: [], touches: [src/app.rs] -->

## 2. Renderer Scale & Orientation

- [ ] 2.1 Modify `render` function in `src/ui.rs` to accept and use `app.scale_factor` and `app.orientation` <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/ui.rs] -->
- [ ] 2.2 Add component geometry transformation: multiply rect dimensions by `scale_factor` <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/ui.rs] -->
- [ ] 2.3 Implement orientation-based panel reflow: portrait = vertical, landscape = horizontal rows <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs] -->
- [ ] 2.4 Ensure `component_rects` are computed after scale/orientation transformation for hit-testing <!-- agent: rusty-engineer.build, depends_on: [2.2, 2.3], touches: [src/ui.rs] -->
- [ ] 2.5 Write integration test that renders sample patch at each scale preset and verifies component_rects <!-- agent: horst-engineer.fast, depends_on: [], touches: [] -->
- [ ] 2.6 Write integration test that renders sample patch in portrait and landscape and verifies layout difference <!-- agent: horst-engineer.fast, depends_on: [], touches: [] -->

## 3. Handler Key Bindings

- [ ] 3.1 Add `+` key binding in `src/handler.rs` to increase scale factor (cycle: 0.5 → 1.0 → 1.5 → 2.0 → 0.5) <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/handler.rs] -->
- [ ] 3.2 Add `-` key binding in `src/handler.rs` to decrease scale factor (cycle: 2.0 → 1.5 → 1.0 → 0.5 → 2.0) <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/handler.rs] -->
- [ ] 3.3 Add `o` key binding in `src/handler.rs` to toggle orientation (Portrait ↔ Landscape) <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/handler.rs] -->
- [ ] 3.4 Update status bar to display current scale and orientation state <!-- agent: rusty-engineer.build, depends_on: [3.1, 3.2, 3.3], touches: [src/handler.rs] -->
- [ ] 3.5 Write unit test for handle_event with scale/orientation key presses and verify state changes <!-- agent: horst-engineer.fast, depends_on: [], touches: [src/handler.rs] -->
- [ ] 3.6 Write integration test that presses `+`/`-`/o and verifies app state and re-render <!-- agent: horst-engineer.fast, depends_on: [], touches: [] -->

## 4. Verification & Cleanup

- [ ] 4.1 Run `cargo build --all-targets --locked` and verify no compilation errors <!-- agent: rusty-engineer.fast, depends_on: [3.1, 3.2, 3.3, 3.4, 3.5, 3.6], touches: [] -->
- [ ] 4.2 Run `cargo test` and verify all 24 existing tests pass <!-- agent: horst-engineer.fast, depends_on: [4.1], touches: [] -->
- [ ] 4.3 Run `cargo clippy --all-targets --all-features --locked -- -D warnings` and verify no warnings <!-- agent: rusty-engineer.fast, depends_on: [4.1], touches: [] -->
- [ ] 4.4 Run `cargo fmt --check` and verify formatting is clean <!-- agent: layout-designer-engineer.fast, depends_on: [4.1], touches: [] -->
- [ ] 4.5 Verify the binary `droid_tui` starts and displays the sample patch correctly <!-- agent: rusty-engineer.fast, depends_on: [4.1, 4.2, 4.3, 4.4], touches: [] -->