## 1. Extract the window-event outcome

- [x] 1.1 Extract a pure window-event outcome from `AppHandler::window_event` in src/main.rs (close request to close-and-exit, keyboard input to `handle_window_event`'s quit bool, modifier change to tracking, redraw to redraw, everything else to egui), with no behavior change <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/main.rs] -->

## 2. Regression tests at the winit seam

- [x] 2.1 Add tests for `handle_window_key_event` over the keybinding map (q quits, l opens the picker, g v opens the viewer, g g opens the graph, 1..4 sets the active shift, e opens the overlay, Esc clears), asserting App state and the return value <!-- agent: horst-engineer.build, depends_on: [], touches: [src/handler.rs] -->
- [x] 2.2 Add a test for `handle_window_event` routing a synthesized `WindowEvent::KeyboardInput` through the same dispatch <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/handler.rs] -->
- [x] 2.3 Add a test for the extracted outcome: close request to close-and-exit, redraw to redraw, q to quit (guards droid_tui-7y5 and droid_tui-5u9) <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/main.rs] -->

## 3. End-to-end flow

- [x] 3.1 Add one egui_kittest end-to-end test that loads a patch, drives converted key events through the handler, repaints, and asserts the quit signal plus a queryable surface by label <!-- agent: horst-engineer.build, depends_on: [2.1, 2.3], touches: [src/regression.rs] -->

## 4. Verification

- [x] 4.1 Run the full verification gate: cargo fmt --check, cargo clippy --all-targets --all-features --locked -- -D warnings, cargo test --locked, cargo build --release --locked <!-- agent: horst-engineer.fast, depends_on: [1.1, 2.1, 2.2, 2.3, 3.1], touches: [] -->
