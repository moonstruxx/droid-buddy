## Why

Since the graph-window-scene-feeding work, `windowed::run` calls `app.open_graph()` unconditionally at startup after `load_initial_patch`. This builds the signal-flow graph and runs `layout::solve` synchronously on the main thread before the first paint. On large patches (melody2 ~532 sections) the OS declares the window unresponsive, and dragging nodes blocks the event loop via the same synchronous path. Users see the graph immediately instead of the physical/panels view and then hang on interaction.

## What Changes

- Remove the unconditional `app.open_graph()` at startup; the app opens on the physical/panels view with no graph until `g g`.
- Keep `load_initial_patch` only; graph building is deferred to the explicit `g g` user action.
- Adjust tests that relied on startup seeding producing a non-empty scene.
- No change to graph layout correctness; only when it runs.

## Capabilities

### New Capabilities
- none

### Modified Capabilities
- none — this is a bug fix restoring the prior startup contract; no spec-level behavior change beyond removing the unintended auto-open.

## Impact

- `src/main.rs` windowed startup sequence
- Tests in `src/main.rs` mirroring startup
- Verification: live visual check (initial view without graph, graph opens on `g g`, drag stays responsive)

## Non-goals

- No async/threaded layout solver; no change to `layout::solve` or `Graph::build_from_patch` internals.
- No new graph-window modality or persistence.
