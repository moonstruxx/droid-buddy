# Wire GPU graph-window request consumption in main.rs windowed loop

## Why

The `gpu-graph-window` change (task 3.2) added the App-side contract: `App::request_graph_window` / `take_graph_window_request` and the `graph_window_enabled` flag seeded from `[gui] graph_window`. main.rs was off-limits for that task, so the windowed run-loop never seeds the flag and `g g` with `graph_window = true` still opens the terminal tile. The configured behavior is specified but not wired.

## What Changes

- `seed_app` (src/main.rs) sets `app.graph_window_enabled = settings.gui.graph_window` under the `gui` feature, matching the physical-view seeding contract.
- The windowed run-loop already consumes the pending request each frame (`pump` → `act_on_window_request` → `take_graph_window_request`, with Open → `open_window_or_tile`, Toggle → open/close by `is_open`, headless fallback to the terminal tile); this change closes the last gap so the configured `g g` behavior actually takes effect.
- A `gui`-gated unit test covers the seeding so the wiring cannot regress.

## Capabilities

### Modified Capabilities
- `configuration`: the `[gui] graph_window` requirement gains an explicit startup-seeding scenario.