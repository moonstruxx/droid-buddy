## Why

The two event-loop fixes shipped this week landed at the winit boundary with no automated tests. Routing keyboard events through the DROID handler and consuming the quit bool (droid_tui-5u9), and exiting the loop on window close (droid_tui-7y5), both live in src/main.rs (`AppHandler::window_event`) and src/handler.rs (`handle_window_event`, `handle_window_key_event`). That boundary is the only layer the test suite does not touch, and both bugs came back after shipping because the only proof was a live Wayland window, which cannot be driven reliably (no xdotool input, focus races) and gave false "still broken" signals.

## What Changes

- Extract the window-event outcome from `AppHandler::window_event` into a pure decision function (close request maps to close-and-exit, keyboard input delegates to the handler and returns its quit flag). Behavior stays identical, but the decision is now assertable without a live `ActiveEventLoop`.
- Add regression tests for `handle_window_key_event` and `handle_window_event` that synthesize winit keys across the keybinding map and assert both the App-state change and the quit result.
- Add one end-to-end egui_kittest test that proves the full chain (key event to handler to App state to repaint) for the load-and-quit flow, reusing the AccessKit labels added in gui-testing-accessibility.

## Capabilities

### New Capabilities

- `gui-flow-verification`: a tested, testable winit event-loop boundary with end-to-end quit and close coverage

## Non-goals

- Re-covering the paint surfaces (already covered by gui-testing-accessibility journey tests)
- Live-screen verification tooling (xdotool, Wayland capture); the new tests replace the need for it
- Snapshot or visual regression (a separate concern)

## Impact

- `src/main.rs`: extract and test the window-event decision
- `src/handler.rs`: test the winit-facing keyboard seams
- `src/regression.rs`: the end-to-end egui_kittest flow
