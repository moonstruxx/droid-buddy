## Context

The winit event boundary is split across src/main.rs (`AppHandler::window_event`, inside `mod windowed`) and src/handler.rs (`handle_window_event` to `handle_window_key_event` to `handle_event`). The former owns close requests, redraw requests, and modifier tracking; the latter owns keyboard to App dispatch and returns a quit bool. Neither seam has tests. Both of this week's event-loop regressions lived here.

## Goals / Non-Goals

**Goals:**

- Make the close and quit decision testable without a live `ActiveEventLoop`
- Cover the winit-facing keyboard seams with regression tests
- Prove the full key to handler to state to repaint chain once, end to end

**Non-Goals:**

- Re-covering the paint surfaces (gui-testing-accessibility journey tests)
- Live Wayland or xdotool verification
- Snapshot visual regression

## Decisions

**D1: Extract a window-event outcome function**

A pure function maps a `WindowEvent` plus tracked modifiers to an outcome enum (continue, redraw, close-and-exit, quit). `AppHandler::window_event` calls it and acts on the result. Close request and quit stay behavior-identical; the test asserts the enum instead of reaching into the event loop.

**D2: Test `handle_window_key_event`, not `handle_event`**

The doc comment already splits the winit conversion (key, element state, modifiers) from the neutral dispatch so tests can drive the conversion without constructing a `KeyEvent`, whose constructor is private. The keybinding map is exercised at this seam because that is where winit input shape becomes quit or state.

**D3: Reuse AccessKit labels for the end-to-end assertion**

The end-to-end test asserts query-by-label rather than pixel snapshots, reusing the annotations from gui-testing-accessibility. This proves render output without a flaky wgpu snapshot under CI.

## Risks / Trade-offs

- **Hot path touched**: extracting the outcome function touches the event loop on every window event. Mitigated by keeping it a pure mapping with no allocation and behavior byte-identical, guarded by the existing main tests plus the new decision test.
- **wgpu snapshot flakiness**: the egui_kittest harness has snapshot and wgpu features enabled. If the pixel path proves flaky in CI, the assertion falls back to the accessibility tree, which is stable.
