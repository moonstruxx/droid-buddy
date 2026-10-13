# Proposal

## Why

The force-directed graph arrangement is unusable on a large **cyclic** patch: it renders a horizontal smear roughly 19.6 million world units wide.

`layout::seed_positions` (`src/layout.rs`) seeds `x = topological_depth[i] * HORIZONTAL_SPACING` from the **raw** depth, and `topological_depth` → `depth_over` is a bounded Bellman-Ford relaxation run for `n` passes. On a cycle, each pass adds about the cycle length, so depth is **not** bounded by `n` — the function's own comment warns that per-depth maps must not assume `depth < n`. On `fixtures/own_scale.ini` (511 nodes, banner chains plus feedback cables) the maximum depth is ≈ **245,266**, so the seed alone spans ≈ **19,621,280** units.

The integrator cannot recover from such a seed: acceleration is `clamp(accel, MAX_DISPLACEMENT = 20)` per component, `velocity = (velocity + a) · FRICTION(0.5)` gives `|velocity| ≤ 40` per iteration, and `SOLVE_ITERATIONS = 60` ⇒ at most ≈ **2400** world units of total travel. Measured result: force mode's bbox width on that fixture is **19,621,300** — a smear ~480× beyond the furthest slice target.

The column arrangement is immune because it dense-normalizes depth into columns `0..N-1` (`dense_columns`) before placing anything; the sections arrangement was fixed separately by seeding from patch order. Only the layered force seed still consumes the raw depth. This is the mechanism behind the standing complaint that big patches cannot be handled in the graph window.

## What Changes

- `seed_positions` **dense-normalizes the layered depth** before multiplying by `HORIZONTAL_SPACING`, mirroring the column path: `x = dense_columns(&topological_depth(graph))[i] * HORIZONTAL_SPACING`.
- The seed's horizontal extent is thereby bounded by `(distinct_depth_values − 1) · HORIZONTAL_SPACING ≤ (n − 1) · 80` — proportional to patch size instead of to cycle length, so it stays inside the integrator's travel budget on any patch.
- **Determinism is preserved**: dense-normalization is a deterministic, order-preserving map, so the D9 contract (same patch + same machine + same tension → identical layout) is unaffected.
- **Contiguous-depth graphs are byte-identical**: when the depth values already form a contiguous `0..N-1`, dense-normalization is the identity, so the existing force-mode golden vectors do not move. Only sparse or cycle-inflated depths change.

## Capabilities

### Modified Capabilities

- `signal-flow-graph`: the **Convergence-based layout** requirement now specifies that the layered seed uses *dense-normalized* layers, so the seed stays bounded on cyclic graphs.

## Impact

- Touched: `src/layout.rs` (`seed_positions` + unit tests). Tests and stories that pin force-mode **positions** on graphs whose depth was sparse or cycle-inflated must be updated to the new contract: `layout::tests`, and force-position assertions in `src/regression.rs` and `src/app.rs`.
- Behaviour: force-mode positions change only where raw depth was sparse/cyclic; the piped/contiguous cases and the column and sections arrangements are untouched. `local_resettle` is unaffected in production (it re-settles from current positions, not from the seed).
- No new config key, no `.ini` mutation, no persistence.

## Non-goals

- No redesign of the force arrangement — no width-aware lanes, no row wrapping, no post-condition passes. That model belongs to `sections` mode and is not retrofitted here.
- No change to the column arrangement (already dense-normalized) or to `sections` mode.
- No change to the integrator constants (`MAX_DISPLACEMENT`, `FRICTION`, `SOLVE_ITERATIONS`) or to the energy-threshold freeze.
- No change to the barycenter crossing-minimization: dense-normalization is an order-preserving bijection on distinct depth values, so the within-layer grouping it consumes is unchanged.
