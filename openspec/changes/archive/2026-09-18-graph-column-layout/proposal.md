## Why

The force-directed graph layout relaxes a layered seed into the final arrangement, so columns drift and wide patches can leave nodes overlapping. nthelper's routing editor solves the same problem with a deterministic layered column layout: depth-based columns, width-aware grid placement, and strict slot stacking, which guarantees no overlap and keeps columns aligned. Adopting it as the default arrangement makes the graph window readable for large patches without giving up the existing force layout.

## What Changes

- New column-layout path in `src/layout.rs`: the existing capped Bellman-Ford depth is dense-normalized to columns `0..N-1`, then nodes are placed width-aware (per-column max width, centered blocks, vertical stacking, `GRID_SNAP`). No force relaxation runs on this path.
- The column layout becomes the default arrangement; the force layout stays reachable via a toggle.
- Within-layer ordering is configurable: strict slot order (default, nthelper style) or the existing barycenter crossing-minimization sweeps.
- Controller and jack nodes sit in fixed outer columns on the column path, with circuit columns between them, mirroring nthelper's physical I/O placement.
- New `[layout]` config table with `mode` (default `column`) and `ordering` (default `strict`) keys.
- Tension (`Alt+[`/`Alt+]`), pins (`p`), and drag re-settle stay on the force path unchanged. The column path ignores tension; pins may act as fixed-position anchors if cheap.
- nthelper's forward/feedback edge split is not adopted; the capped Bellman-Ford depth stays.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `signal-flow-graph`: the layout section changes from force-directed convergence to a deterministic column arrangement as the default, with a configurable within-layer ordering and a retained force-layout toggle. Node drag and topology behavior are unchanged.
- `configuration`: the config file gains a `[layout]` table (`mode`, `ordering`) with validation and defaults, alongside the existing tables.

## Impact

- `src/layout.rs`: new column-placement function (layered seed, dense normalization, width-aware stacking); force path (`run_iterations`, `local_resettle`) untouched. Node-size estimation added (estimator in the solver or sizes passed in).
- `src/graph_render.rs`: node-size estimation consumed by the grid placement.
- `src/app.rs`: layout-mode state, default wiring, toggle, pin handling on the column path.
- `src/handler.rs`: keybinding and status for the layout-mode toggle; force-path keys unchanged.
- `src/config.rs`: `[layout]` table load/save/clamp, warn-once on malformed values.
- Tests: column-assignment/no-overlap/determinism unit tests in `layout.rs`; snapshot and drag-interaction updates where the default arrangement changes positions.
- Docs: ARCHITECTURE.md and DESIGN.md updated.

## Non-goals

- nthelper's forward/feedback edge split and pull loop (kept out by decision; capped Bellman-Ford stays).
- Changing the force path's behavior, keys, or determinism contract.
- Cascade layout, overlap severity reporting, or per-preset position persistence (nthelper extras with no current consumer).
- Changing node-drag or topology-validation behavior.