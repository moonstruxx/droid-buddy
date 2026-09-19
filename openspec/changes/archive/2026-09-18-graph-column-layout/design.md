## Context

The graph layout solver (`src/layout.rs`) currently seeds positions by topological layer, then relaxes them with force iterations (`run_iterations`, springs, grid-hashed repulsion, friction, energy-threshold freeze) and supports a damped local re-settle on node drag (`local_resettle`). Node sizes are unknown to the solver; the renderer computes them. The proposal (see proposal.md) adds a deterministic column arrangement as the default, modeled on nthelper's auto-arrange, while keeping the force path intact as a toggle.

Design-level constraints that shape the approach:

- The solver is a pure deterministic function: same patch, same machine, same layout. Determinism is a tested contract (layout tests, snapshot tests).
- The force path is user-facing and tested: tension keys (`Alt+[`/`Alt+]`), pin/unpin (`p`), drag auto-pin and local re-settle. It stays behaviorally unchanged.
- Node drag on the graph surface is hit-tested via `graph_node_rects` and calls `local_resettle`. The column arrangement has no re-settle semantics unless pins gain fixed-position meaning.
- Config tables live in `src/config.rs` with warn-once and clamped defaults; `[labels]`, `[latency]`, `[plugins]` are the precedents.
- TDD London: tests go through public entry points (`solve`, `handle_event`, render). The renderer publishes node rects per frame; the solver stays size-agnostic unless sizes are passed in.

## Goals / Non-Goals

**Goals:**

- A column arrangement in `layout.rs` that is deterministic, dense-normalized to `0..N-1` columns, width-aware (no overlap within a column), and grid-snapped.
- The column arrangement is the default; the force arrangement stays reachable via a toggle, with the choice persisting per session.
- Configurable within-column ordering: strict slot order (default) or barycenter crossing-minimization.
- Controller and jack nodes in fixed outer columns on the column path.
- No behavioral change to the force path, its keys, or its determinism contract.

**Non-Goals:**

- Adopting nthelper's forward/feedback edge split and pull loop (the capped Bellman-Ford depth stays).
- Changing node-drag re-settle behavior on the force path.
- Cascade layout, overlap severity reporting, or per-preset position persistence.
- Making the column arrangement compute exact node sizes from the renderer (an estimator in the solver is sufficient; rendering remains size-authoritative).

## Decisions

### D1: Column arrangement as a separate function, force path untouched

`layout::solve` gains a mode. The column arrangement is a new function (for example `solve_columns`) that shares the existing depth computation and grid-snap constants but never runs `run_iterations`. `solve` dispatches by the mode from `App`. Rationale: the force path and its tests stay byte-identical; the new path is independently testable.

Alternatives considered: a rewrite of `solve` in place (rejected: breaks force-path tests and the tension/pin contract), and a separate module (rejected: layout.rs already owns the solver; the column path reuses its depth and constants).

### D2: Column assignment from the existing capped Bellman-Ford depth

Columns come from the current depth computation (capped Bellman-Ford), then dense-normalized to `0..N-1`. Rationale: the annotation decision keeps the capped Bellman-Ford; nthelper's forward/feedback pull loop is out of scope. Cycle handling stays as it is today (bounded growth).

### D3: Width-aware placement with a size estimator inside the solver

The solver estimates each node's width from the circuit name length plus port label lengths (the same inputs the renderer uses for its frame). Per-column width is the max over members; columns center as blocks; nodes stack vertically with fixed spacing, snapped to `GRID_SNAP`.

Alternatives considered: passing exact sizes from the renderer into the solver (rejected: couples the pure solver to the paint layer and breaks the renderer-publishes-geometry handoff), and using a uniform width (rejected: defeats the no-overlap improvement on wide nodes).

### D4: Fixed outer columns for controller and jack nodes

On the column path, controller nodes go to a left outer column and jack nodes to outer columns by direction (inputs left, outputs right), with circuit columns between, mirroring nthelper's physical I/O placement. Circuit depth columns are computed with the outer columns excluded.

### D5: Mode and ordering in `[layout]` config, toggle key in the handler

`config.toml` gains `[layout] mode` and `[layout] ordering`, following the `[labels]` precedent (warn-once, fall back to defaults, clamp on save). `App` holds the active mode, seeded from config at startup, switchable by a keybinding (a `l`-adjacent or graph-pane key; the exact key is resolved during implementation against the existing keybinding spec). The force path's keys keep their meaning on the force arrangement only.

### D6: Pins on the column path

Pins act as fixed-position anchors on the column path: a pinned node keeps its current position while the remaining nodes arrange around it. The force path keeps today's pinned-anchor semantics. If the fixed-position wiring proves expensive, the fallback is that pins are ignored on the column path and the toggle to force restores them; the spec allows the fallback but the fixed-position form is preferred.

## Risks / Trade-offs

- [Column layout is narrower than the force layout on small windows] → The toggle to the force arrangement stays available; the camera fit already handles overflow.
- [The size estimator diverges from renderer-computed sizes, so two nodes could still collide if the renderer draws wider than estimated] → Rendering remains size-authoritative; keep the estimator on the same inputs (name length, port labels) the renderer uses, and verify with a no-overlap property test against the rendered rects.
- [Adding a mode to `solve` widens its signature and every call site] → Keep the mode on the app/options struct so solver call sites stay unchanged where possible.
- [Barycenter ordering changes the vertical order but not columns; crossing counts can still be high on cyclic patches] → Accepted; strict slot order is the default and the crossing-minimization sweeps are the pre-existing mechanism.

## Migration Plan

No migration: this is a pure additive change. The default arrangement changes from force to column on the next load; existing saved config is unchanged (the `[layout]` table is absent, so defaults apply). Rollback is the toggle to the force arrangement or deleting the `[layout]` table.

## Open Questions

None that would change the specs or the task breakdown. The exact toggle keybinding is resolved during implementation against the keybinding spec.