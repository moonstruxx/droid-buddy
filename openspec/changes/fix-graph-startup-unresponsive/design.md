# Design: fix-graph-startup-unresponsive

## Context

`windowed::run` currently does `load_initial_patch` then `app.open_graph()` before the first frame. `open_graph` does `Graph::build_from_patch` + `layout::solve` synchronously. For melody2 this was measured at tens of seconds, blocking the winit event loop so the OS shows "not responding" and every drag frame blocks again. The physical/panels view should be the startup view; the graph is an explicit `g g` slot.

## Approach

- Delete the `app.open_graph()` call in `windowed::run` (src/main.rs:231) and its comment.
- Update the `startup_seeding_produces_a_nonempty_scene` test to assert the startup scene is `None`/empty rather than non-empty, or remove it in favor of a test that explicitly calls `open_graph` after load.
- Keep `seed_app` and `load_initial_patch` unchanged. No thread or async change.
- Drag path (`layout::local_resettle`) stays synchronous; it is bounded (≤40 iterations, local radius) and not the startup-scale cost.

## Alternatives considered

- Background thread for layout: heavier, not needed for this fix; defer building entirely until requested.
- Precompute layout at build time: not applicable.

## Risks

- Tests that assumed startup built a graph will need update — low risk.
- No spec change; guard against reintroducing startup auto-open in future merges.
