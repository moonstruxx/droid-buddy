# Design

## Context

See `proposal.md` for the mechanism and the measured numbers. The constraint that shapes the fix: the force integrator can travel at most ≈2400 world units from its seed (`MAX_DISPLACEMENT` 20 per component, `FRICTION` 0.5 ⇒ `|v| ≤ 40` per iteration, `SOLVE_ITERATIONS` 60). **The seed therefore determines the layout's order of magnitude** — a seed that is 19.6 million units wide cannot be repaired by relaxation.

An index of the code involved:

- `seed_positions` (`src/layout.rs`) — the layered seed: `x = depth[i] · HORIZONTAL_SPACING`, `y = order[i] · VERTICAL_SPACING`.
- `topological_depth` → `depth_over` — capped Bellman-Ford longest-path depth over all edges, `n` passes; unbounded in value on cycles.
- `dense_columns` — maps depth values to dense `0..N-1` ranks (already used by the column arrangement).
- `crossing_minimized_orders` — deterministic barycenter sweeps; consumes depth only as a **layer partitioning**.
- `solve` — force mode: `seed_positions` then `run_iterations` (Force). `solve_sections` seeds from `seed_positions_sections` and is out of scope.

## Goals / Non-Goals

Goals: bound the force seed so it stays inside the integrator's travel budget on any patch, including cyclic ones; keep the D9 determinism contract; leave contiguous-depth layouts byte-identical.

Non-goals: re-modeling the force arrangement (width-aware lanes, wrapping, overlap post-conditions are `sections` mode's design and are deliberately **not** back-ported); touching the integrator constants; touching the column or sections paths.

## Decisions

### D1 — Dense-normalize the layered depth inside `seed_positions`

`seed_positions` maps the raw depth through `dense_columns` before multiplying:

```rust
let depth = dense_columns(&topological_depth(graph));
let order = crossing_minimized_orders(graph, &depth);
let x = depth[i] as f32 * HORIZONTAL_SPACING;
```

**Why dense-normalization rather than a cap.** A cap (e.g. `min(depth, n)`) keeps the *value* bounded but leaves the *spacing* arbitrary: two nodes one cycle apart could still be `n · 80` apart, so the layout would stay an unreachable smear while looking "bounded". Dense-normalization instead guarantees **one spacing unit per distinct layer**, so the seed's x extent is exactly `(distinct_layers − 1) · 80` — proportional to patch size and comfortably inside the 2400-unit reach (a 511-node patch yields at most 40,800 total, with ≤80 between adjacent layers).

**Why it is safe for the other consumers.** `crossing_minimized_orders` uses depth only to partition nodes into layers; dense-normalization is an order-preserving bijection on distinct depth values, so it yields **identical layers and identical within-layer orders**. On graphs whose depth values already form a contiguous `0..N-1`, `dense_columns` is the identity, so such layouts (including the existing force-mode golden vectors, which use a 5-node chain) are **byte-identical**.

**Alternatives considered.**
- *Cap the depth at `n`.* Rejected: bounds the value, not the spacing; still unreachable on cycles (see above).
- *Seed force mode from patch order like `sections` mode.* Rejected as scope creep: it changes the arrangement's visual model (the force path is a signal-flow chain, not a patch-order lane grid) and would invalidate far more pinned positions for no additional correctness.
- *Raise `SOLVE_ITERATIONS` / `MAX_DISPLACEMENT` so the solver could recover.* Rejected: it scales the cost for every patch to paper over one bad seed, weakens the convergence guarantee, and does not remove the pathological seed.
- *Give the force path width-aware wrapping.* Rejected: that is `sections` mode; duplicating it would leave two near-identical arrangements.

**Blast radius.** Force-mode **positions** change only where raw depth was sparse or cycle-inflated. Pinned expectations must be updated where that is true: unit tests in `layout::tests` that call `seed_positions`/`solve` on such graphs, and force-position assertions in `src/regression.rs` and `src/app.rs`. Contiguous-depth cases stay byte-identical and must be shown to be so.

**Determinism.** `dense_columns` sorts and dedups a value list — deterministic, no RNG, no hash iteration. The D9 contract (same patch + same machine + same tension → identical positions) holds unchanged, per tension value.

## Risks / Trade-offs

- **A repinned golden vector can hide a regression.** Mitigation: the update is justified by measurement — report the before/after numbers for each changed expectation, and keep at least one contiguous-depth golden vector untouched to prove the identity case.
- **Sparse-but-acyclic graphs also shift.** This is intended (one spacing unit per distinct layer is the contract), but it means more expectations move than "cyclic only". State it per test.
- **The seed is still a single dominant axis with `n` layers in the worst case** (≤40,800 wide on the scale fixture). The force arrangement remains visually narrower-quality than `sections` mode on huge patches; that is accepted — this change fixes the *explosion*, not the arrangement's density.

## Migration

None: no config, no persistence, no data format. Pure layout-function behaviour.
