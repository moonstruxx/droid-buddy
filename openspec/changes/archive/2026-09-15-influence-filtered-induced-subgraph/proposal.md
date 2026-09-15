## Why

The `modifier-trace` spec already requires a filtered induced subgraph for an active modifier influence, but the app only highlights influenced nodes/edges on the full graph (`graph.highlighted_nodes/edges`). A patch with hundreds of circuits keeps the full layout on screen, so the influenced region stays hard to read. Building the induced subgraph as its own solvable graph closes that gap.

## What Changes

- Add a pure induced-subgraph builder: filter a `Graph` to `influenced_nodes`, keep edges whose endpoints are both influenced, inherit banner-cluster membership.
- Solve the subset as its own layout with subset-relative pins (mirroring `apply_dependency_subset`), with a separate `GraphCamera` fit.
- Wire the subset to a filtered view toggle on the graph surface; clearing restores the full-graph solve.
- Keep the existing FULL-graph highlight path untouched.

## Capabilities

### New Capabilities

None. This implements the existing `modifier-trace` "Filtered induced subgraph" requirement; no spec-level behavior changes.

### Modified Capabilities

None for the same reason. The change sets `skip_specs: true`.

## Impact

- `src/graph.rs`: new pure builder + unit tests.
- `src/app.rs`: subset state, solve, rebuild integration.
- `src/gui/graph.rs`: filtered render path + camera fit.
- `src/handler.rs`: toggle key wiring + tests.
- No schema, config, or persistence changes.

## Non-goals

- No change to the influence walk itself (`patch.rs` untouched).
- No change to the dependency filter (`f`); the two filters stay independent.
- No persistence of the filtered view across patch loads.
